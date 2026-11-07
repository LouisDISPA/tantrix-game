//! A room's lifecycle as both servers drive it: records, codes, presence
//! and sockets through one `RoomService`, over a fake host that records
//! what it is asked to store and deliver.

mod support;

use std::collections::{HashMap, HashSet};

use support::{closes, hello, now, token};
use tantrix_game::{Color, Game, Player};
use tantrix_proto::{HostMessage, PeerId, PeerMessage, RoomApiError, RoomReference};
use tantrix_room::engine::{ConnId, Connection, Effect, STORAGE_UNAVAILABLE, TOO_SLOW};
use tantrix_room::lifecycle::{
    Catalog, Invitation, RoomRecord, IDLE_TTL_MS, RETENTION_MS, TOMBSTONE_TTL_MS,
};
use tantrix_room::room_logic::{CompletedMatch, RoomData};
use tantrix_room::service::{Cleanup, RoomService, ServiceHost, LOBBY_EXPIRED, ROOM_DELETED};
use tantrix_room::session_cookie::SessionToken;

const ID: &str = "01234567-89ab-4def-8123-456789abcdef";

/// Storage, the code catalog and sockets, in memory.
#[derive(Default)]
struct FakeHost {
    /// The record as storage holds it; `None` once deleted.
    stored: Option<RoomRecord>,
    archives: Vec<CompletedMatch>,
    /// Code → (room ID, claim nonce).
    claims: HashMap<String, (String, String)>,
    /// Another room holds every code.
    codes_taken: bool,
    fail_saves: bool,
    /// Sockets that can't take a message any more.
    dead: HashSet<ConnId>,
    /// What the host was asked to do, in order.
    ops: Vec<&'static str>,
    delivered: Vec<Effect>,
    cleanups: Vec<Cleanup>,
}

impl FakeHost {
    fn stored(&self) -> &RoomRecord {
        self.stored.as_ref().expect("a stored record")
    }

    /// How many saves (archives included) since the last call.
    fn saves(&mut self) -> usize {
        let saves = self
            .ops
            .iter()
            .filter(|op| matches!(**op, "save" | "archive"))
            .count();
        self.ops.clear();
        saves
    }

    fn take(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.delivered)
    }

    /// Forget what was asked so far.
    fn reset(&mut self) {
        self.ops.clear();
        self.delivered.clear();
    }
}

impl Catalog for FakeHost {
    async fn save(&mut self, record: &RoomRecord) -> Result<(), RoomApiError> {
        self.save_record(record, None).await
    }

    async fn claim(&mut self, id: &str, invitation: &Invitation) -> Result<bool, RoomApiError> {
        if self.codes_taken {
            return Ok(false);
        }
        let owner = self
            .claims
            .entry(invitation.code.clone())
            .or_insert_with(|| (id.into(), invitation.claim.clone()));
        Ok(*owner == (id.into(), invitation.claim.clone()))
    }

    async fn release(&mut self, id: &str, invitation: &Invitation) -> Result<(), RoomApiError> {
        if self.claims.get(&invitation.code) == Some(&(id.into(), invitation.claim.clone())) {
            self.claims.remove(&invitation.code);
        }
        Ok(())
    }

    async fn delete(&mut self, _id: &str) -> Result<(), RoomApiError> {
        self.stored = None;
        Ok(())
    }
}

impl ServiceHost for FakeHost {
    type Catalog = Self;

    fn catalog(&mut self) -> &mut Self {
        self
    }

    async fn save_record(
        &mut self,
        record: &RoomRecord,
        archive: Option<CompletedMatch>,
    ) -> Result<(), RoomApiError> {
        if self.fail_saves {
            return Err(RoomApiError::StorageUnavailable);
        }
        self.stored = Some(record.clone());
        self.ops
            .push(if archive.is_some() { "archive" } else { "save" });
        self.archives.extend(archive);
        Ok(())
    }

    fn deliver(&mut self, effects: Vec<Effect>) -> Vec<ConnId> {
        let mut unreachable = Vec::new();
        for effect in &effects {
            let (op, to) = match effect {
                Effect::Send { to, .. } => ("send", vec![*to]),
                Effect::Broadcast { to, .. } => ("broadcast", to.clone()),
                Effect::Close { .. } => ("close", Vec::new()),
            };
            self.ops.push(op);
            unreachable.extend(to.into_iter().filter(|conn| self.dead.contains(conn)));
        }
        self.delivered.extend(effects);
        unreachable
    }

    fn cleaned(&mut self, cleanup: Cleanup) {
        self.cleanups.push(cleanup);
    }
}

struct Room {
    service: RoomService,
    host: FakeHost,
    /// Milliseconds. Each frame is sent a second after the last.
    clock: u64,
}

impl Room {
    /// A new lobby, created and loaded at time 0.
    fn new() -> Self {
        Self::stored(RoomRecord::new(ID.into(), 0), [])
    }

    /// `record` read from storage with `connections` still open, loaded.
    fn stored(
        record: RoomRecord,
        connections: impl IntoIterator<Item = (ConnId, Connection)>,
    ) -> Self {
        let mut room = Self::unloaded(record, connections);
        now(room.service.load(&mut room.host, 0)).unwrap();
        room.host.reset();
        room
    }

    fn unloaded(
        record: RoomRecord,
        connections: impl IntoIterator<Item = (ConnId, Connection)>,
    ) -> Self {
        Self {
            service: RoomService::new(record, connections),
            host: FakeHost::default(),
            clock: 0,
        }
    }

    fn inspect(&mut self, allocate: bool, at: u64) -> Result<RoomReference, RoomApiError> {
        self.clock = at;
        now(self.service.inspect(&mut self.host, allocate, at))
    }

    fn open(&mut self, conn: u64, token: &SessionToken) -> Result<(), RoomApiError> {
        now(self
            .service
            .open(&mut self.host, ConnId(conn), token.clone(), self.clock))
    }

    fn send(&mut self, conn: u64, message: &PeerMessage) -> Result<(), RoomApiError> {
        self.clock += 1000;
        let text = serde_json::to_string(message).unwrap();
        now(self
            .service
            .text(&mut self.host, ConnId(conn), &text, self.clock))
    }

    fn close(&mut self, conn: u64) -> Result<(), RoomApiError> {
        self.clock += 1000;
        now(self.service.close(&mut self.host, ConnId(conn), self.clock))
    }

    /// Open `conn` for `name` and say hello.
    fn join(&mut self, conn: u64, name: &str) -> SessionToken {
        let token = token(name);
        self.open(conn, &token).unwrap();
        self.send(conn, &hello(name)).unwrap();
        token
    }

    fn pick(&mut self, conn: u64, color: Color) -> Result<(), RoomApiError> {
        self.send(conn, &PeerMessage::PickColor { color: Some(color) })
    }

    fn room(&self) -> &RoomData {
        self.service.record().room.as_ref().expect("a live room")
    }
}

/// A stored room with these members, saved while someone was connected.
fn members(names: &[&str]) -> RoomRecord {
    let mut room = RoomData::default();
    for name in names {
        room.hello(token(name), (*name).into(), PeerId::new(*name))
            .unwrap();
    }
    let mut record = RoomRecord::new(ID.into(), 0);
    record.room = Some(room);
    record.observe_connections(true, 0);
    record
}

fn joined(name: &str) -> Connection {
    Connection {
        token: token(name),
        joined: true,
    }
}

/// A record whose first match is under way.
fn started() -> RoomRecord {
    let mut record = RoomRecord::new(ID.into(), 0);
    let game = Game::new_seeded(
        Player::Someone,
        Player::Someone,
        Player::None,
        Player::None,
        42,
    )
    .unwrap();
    record.room.as_mut().unwrap().game = Some(game.to_data());
    record
}

#[test]
fn loading_settles_the_room_and_schedules_its_cleanup() {
    // Saved while Red, seated, and a watcher were connected.
    let mut record = members(&["Red", "Watcher"]);
    record.room.as_mut().unwrap().seats[Color::Red] = Some(token("Red"));

    // After a restart no socket is open: the watcher leaves, Red keeps the
    // seat, and an inactivity period starts.
    let mut restarted = Room::unloaded(record.clone(), []);
    now(restarted.service.load(&mut restarted.host, 5)).unwrap();
    assert_eq!(restarted.host.saves(), 2, "the room, then the record");
    let stored = restarted.host.stored();
    let room = stored.room.as_ref().unwrap();
    assert!(room.player(&token("Watcher")).is_none());
    assert_eq!(room.color_of(&token("Red")), Some(Color::Red));
    assert_eq!(stored.vacant_since, Some(5));
    assert_eq!(stored.deadline(), Some(5 + IDLE_TTL_MS));

    // Woken with the watcher's socket still open (hibernation), nobody
    // leaves; the record is saved anyway, which schedules its alarm.
    let mut woken = Room::unloaded(record, [(ConnId(2), joined("Watcher"))]);
    now(woken.service.load(&mut woken.host, 5)).unwrap();
    assert_eq!(woken.host.saves(), 1);
    assert!(woken.room().player(&token("Watcher")).is_some());
    assert_eq!(woken.host.stored().vacant_since, None);
}

#[test]
fn loading_saves_the_room_as_empty_when_its_last_socket_turns_out_dead() {
    // The watcher's socket outlived hibernation but can't take the roster
    // the load sends when the other watcher, long gone, leaves.
    let record = members(&["Gone", "Watcher"]);
    let mut woken = Room::unloaded(record, [(ConnId(2), joined("Watcher"))]);
    woken.host.dead.insert(ConnId(2));
    now(woken.service.load(&mut woken.host, 5)).unwrap();

    assert_eq!(closes(&woken.host.take()), [(2, TOO_SLOW)]);
    let stored = woken.host.stored();
    assert!(stored.room.as_ref().unwrap().players.is_empty());
    assert_eq!(stored.vacant_since, Some(5), "nobody is connected");
    assert_eq!(stored.deadline(), Some(5 + IDLE_TTL_MS));
}

#[test]
fn a_socket_hears_the_room_reference_before_anything_else() {
    let mut room = Room::new();
    let reference = room.inspect(true, 0).unwrap();
    assert!(reference.code.is_some());
    room.host.reset();
    room.open(1, &token("Red")).unwrap();
    let delivered = room.host.take();
    assert!(
        matches!(&delivered[..], [Effect::Send { to: ConnId(1), message: HostMessage::RoomInfo { room } }] if *room == reference)
    );
    assert_eq!(room.host.saves(), 0, "an open socket changes nothing");
}

#[test]
fn presence_is_saved_only_when_it_changes() {
    let mut room = Room::new();
    room.open(1, &token("Red")).unwrap();
    assert_eq!(room.host.saves(), 0);

    // The new member is saved, then that someone is connected.
    room.send(1, &hello("Red")).unwrap();
    assert_eq!(room.host.saves(), 2);
    assert_eq!(room.host.stored().vacant_since, None);

    // An answer to the sender changes neither.
    room.send(1, &PeerMessage::RequestSnapshot).unwrap();
    assert_eq!(room.host.saves(), 0);

    // Red, without a seat, leaves; nobody is connected from then on.
    room.close(1).unwrap();
    assert_eq!(room.host.saves(), 2);
    assert_eq!(room.host.stored().vacant_since, Some(room.clock));
    assert!(room.room().players.is_empty());
}

#[test]
fn changes_are_saved_and_archived_before_anyone_hears_of_them() {
    let mut room = Room::new();
    room.join(1, "Red");
    room.join(2, "Blue");
    room.pick(1, Color::Red).unwrap();
    room.pick(2, Color::Blue).unwrap();
    room.send(1, &PeerMessage::StartGame { match_id: 0 })
        .unwrap();
    room.send(2, &PeerMessage::EndGame { match_id: 0 }).unwrap();
    room.host.reset();

    room.send(1, &PeerMessage::Rematch { match_id: 0 }).unwrap();
    assert_eq!(room.host.ops, ["archive", "broadcast"]);
    assert_eq!(room.host.archives.len(), 1);
    assert_eq!(room.host.archives[0].id, 0);
    let stored = room.host.stored().room.clone().unwrap();
    assert_eq!(stored.match_id, 1);
    assert!(stored.game.is_none());
    assert_eq!(room.room().match_id, 1);
}

#[test]
fn a_failed_save_leaves_the_record_as_storage_has_it_and_closes_every_socket() {
    let mut room = Room::new();
    room.join(1, "Red");
    room.join(2, "Watcher");
    let before = serde_json::to_value(room.service.record()).unwrap();
    room.host.reset();

    room.host.fail_saves = true;
    assert_eq!(
        room.pick(1, Color::Red),
        Err(RoomApiError::StorageUnavailable)
    );
    assert_eq!(
        serde_json::to_value(room.service.record()).unwrap(),
        before,
        "nothing uncommitted for a later save to write"
    );
    assert_eq!(
        closes(&room.host.take()),
        [(1, STORAGE_UNAVAILABLE), (2, STORAGE_UNAVAILABLE)]
    );
    assert_eq!(
        room.service.closed(),
        Some(RoomApiError::StorageUnavailable)
    );
    assert!(room.service.engine().is_none());

    // Spent: the adapter loads the room again for the next visitor.
    room.host.fail_saves = false;
    assert_eq!(
        room.inspect(true, room.clock),
        Err(RoomApiError::StorageUnavailable)
    );
    assert_eq!(
        room.open(3, &token("Late")),
        Err(RoomApiError::StorageUnavailable)
    );
    assert_eq!(closes(&room.host.take()), [(3, STORAGE_UNAVAILABLE)]);
    assert_eq!(room.host.saves(), 0);
}

#[test]
fn a_presence_that_cannot_be_saved_closes_every_socket() {
    let mut room = Room::new();
    let red = room.join(1, "Red");
    room.pick(1, Color::Red).unwrap();
    room.close(1).unwrap();
    room.open(2, &token("Waiting")).unwrap();
    room.host.reset();

    // Red resumes the seat: the room is unchanged, only presence is new.
    room.host.fail_saves = true;
    room.open(3, &red).unwrap();
    assert_eq!(
        room.send(3, &hello("Red")),
        Err(RoomApiError::StorageUnavailable)
    );
    let delivered = room.host.take();
    assert_eq!(
        closes(&delivered),
        [(2, STORAGE_UNAVAILABLE), (3, STORAGE_UNAVAILABLE)]
    );
    assert_eq!(
        room.service.closed(),
        Some(RoomApiError::StorageUnavailable)
    );
}

#[test]
fn a_room_that_cannot_be_loaded_closes_the_sockets_that_outlived_hibernation() {
    let record = members(&["Watcher"]);
    let mut woken = Room::unloaded(record, [(ConnId(4), joined("Watcher"))]);
    woken.host.fail_saves = true;
    assert_eq!(
        now(woken.service.load(&mut woken.host, 0)),
        Err(RoomApiError::StorageUnavailable)
    );
    assert_eq!(closes(&woken.host.take()), [(4, STORAGE_UNAVAILABLE)]);
    assert_eq!(
        woken.service.closed(),
        Some(RoomApiError::StorageUnavailable)
    );
}

#[test]
fn an_expired_lobby_closes_its_sockets_and_stays_a_tombstone_until_deleted() {
    let mut room = Room::new();
    let code = room.inspect(true, 0).unwrap().code.unwrap();
    // A socket that never said hello keeps nobody connected.
    room.open(1, &token("Silent")).unwrap();
    room.host.reset();

    assert_eq!(room.inspect(false, IDLE_TTL_MS), Err(RoomApiError::Expired));
    assert_eq!(closes(&room.host.take()), [(1, LOBBY_EXPIRED)]);
    assert_eq!(room.host.cleanups, [Cleanup::LobbyExpired]);
    assert!(!room.host.claims.contains_key(&code), "code released");
    assert!(room.host.stored().room.is_none());
    assert_eq!(room.service.closed(), None, "a tombstone is kept");
    room.host.reset();

    // The tombstone turns sockets away and ignores frames.
    assert_eq!(room.inspect(true, IDLE_TTL_MS), Err(RoomApiError::Expired));
    room.open(2, &token("Late")).unwrap();
    room.send(2, &hello("Late")).unwrap();
    assert_eq!(closes(&room.host.take()), [(2, LOBBY_EXPIRED)]);
    assert_eq!(room.host.saves(), 0);

    let deletion = IDLE_TTL_MS + TOMBSTONE_TTL_MS;
    assert_eq!(room.inspect(false, deletion), Err(RoomApiError::NotFound));
    assert!(room.host.stored.is_none(), "deleted");
    assert_eq!(
        room.host.cleanups[1..],
        [Cleanup::Deleted { tombstone: true }]
    );
    assert_eq!(room.service.closed(), Some(RoomApiError::NotFound));
}

#[test]
fn a_started_room_releases_its_code_then_retention_deletes_it() {
    let mut room = Room::stored(started(), []);
    room.inspect(true, 0).unwrap();
    let before = serde_json::to_value(room.room()).unwrap();

    let kept = room.inspect(false, IDLE_TTL_MS).unwrap();
    assert_eq!(kept.code, None);
    assert_eq!(room.host.cleanups, [Cleanup::CodeReleased]);
    assert!(room.host.claims.is_empty());
    assert_eq!(serde_json::to_value(room.room()).unwrap(), before);

    room.open(1, &token("Silent")).unwrap();
    room.host.reset();
    assert_eq!(
        room.inspect(false, RETENTION_MS),
        Err(RoomApiError::NotFound)
    );
    assert!(room.host.stored.is_none(), "deleted");
    assert_eq!(closes(&room.host.take()), [(1, ROOM_DELETED)]);
    assert_eq!(
        room.host.cleanups[1..],
        [Cleanup::Deleted { tombstone: false }]
    );
    assert_eq!(room.open(2, &token("Late")), Err(RoomApiError::NotFound));
    assert_eq!(closes(&room.host.take()), [(2, ROOM_DELETED)]);
}

#[test]
fn a_resumed_game_gets_a_new_code_and_a_whole_inactivity_period() {
    let mut room = Room::stored(started(), []);
    let original = room.inspect(true, 0).unwrap();
    let before = serde_json::to_value(room.room()).unwrap();
    assert_eq!(room.inspect(false, IDLE_TTL_MS).unwrap().code, None);

    let resumed = room.inspect(true, IDLE_TTL_MS + 1).unwrap();
    assert_eq!(resumed.id, original.id);
    assert!(resumed.code.is_some());
    let stored = room.host.stored();
    assert_eq!(stored.deadline(), Some(2 * IDLE_TTL_MS + 1));
    assert_eq!(serde_json::to_value(stored.room.as_ref()).unwrap(), before);
}

#[test]
fn a_started_game_is_served_without_a_code_when_none_is_free() {
    let mut lobby = Room::new();
    lobby.host.codes_taken = true;
    assert_eq!(lobby.inspect(true, 0), Err(RoomApiError::AllocationBusy));
    assert_eq!(lobby.service.closed(), None);
    assert!(lobby.service.engine().is_some());

    let mut game = Room::stored(started(), []);
    game.host.codes_taken = true;
    let reference = game.inspect(true, 0).unwrap();
    assert_eq!(reference.id, ID);
    assert_eq!(reference.code, None);
}
