//! The room engine as both servers drive it: socket events in, effects out,
//! through a fake host that records saves and deliveries.

use std::collections::HashSet;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use tantrix_game::Color;
use tantrix_proto::{HostMessage, PeerMessage, RejectCode};
use tantrix_room::engine::{
    ConnId, Connection, Effect, RoomEngine, RoomHost, Step, MAX_MESSAGE_BYTES, OPENED_ELSEWHERE,
    STORAGE_UNAVAILABLE, TOO_SLOW,
};
use tantrix_room::room_logic::{RoomData, MAX_MEMBERS};
use tantrix_room::session_cookie::SessionToken;

/// Records what the engine asks of its server.
#[derive(Default)]
struct FakeHost {
    saved: Vec<RoomData>,
    fail_saves: bool,
    delivered: Vec<Effect>,
    /// Sockets whose queue is full: anything sent to them is reported back.
    unreachable: HashSet<ConnId>,
}

impl RoomHost for FakeHost {
    type Error = &'static str;

    fn save(&mut self, room: &RoomData) -> impl Future<Output = Result<(), &'static str>> {
        let result = if self.fail_saves {
            Err("storage is down")
        } else {
            self.saved.push(room.clone());
            Ok(())
        };
        std::future::ready(result)
    }

    fn deliver(&mut self, effects: Vec<Effect>) -> Vec<ConnId> {
        let mut unreachable = Vec::new();
        for effect in &effects {
            for conn in message_recipients(effect) {
                if self.unreachable.contains(&conn) && !unreachable.contains(&conn) {
                    unreachable.push(conn);
                }
            }
        }
        self.delivered.extend(effects);
        unreachable
    }
}

fn message_recipients(effect: &Effect) -> Vec<ConnId> {
    match effect {
        Effect::Send { to, .. } => vec![*to],
        Effect::Broadcast { to, .. } => to.clone(),
        Effect::Close { .. } => Vec::new(),
    }
}

/// The fake host never waits, so one poll finishes a run.
fn now<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("the fake host never waits"),
    }
}

struct Room {
    engine: RoomEngine,
    host: FakeHost,
}

impl Room {
    fn new() -> Self {
        Self::loaded(RoomData::default(), [])
    }

    fn loaded(room: RoomData, connections: impl IntoIterator<Item = (ConnId, Connection)>) -> Self {
        Self {
            engine: RoomEngine::new(room, connections),
            host: FakeHost::default(),
        }
    }

    /// Run `step` and return what it delivered (kept on the host if the
    /// run fails).
    fn run(&mut self, step: Step) -> Result<Vec<Effect>, &'static str> {
        now(self.engine.run(&mut self.host, step))?;
        Ok(std::mem::take(&mut self.host.delivered))
    }

    /// Run `step` through a failing save; return the error and what it
    /// delivered.
    fn run_failing(&mut self, step: Step) -> (&'static str, Vec<Effect>) {
        let error = self.run(step).expect_err("the save fails");
        (error, std::mem::take(&mut self.host.delivered))
    }

    fn open(&mut self, conn: u64, token: &SessionToken) -> Vec<Effect> {
        let step = self.engine.on_open(ConnId(conn), token.clone());
        self.run(step).unwrap()
    }

    fn send(&mut self, conn: u64, message: &PeerMessage) -> Vec<Effect> {
        self.text(conn, &serde_json::to_string(message).unwrap())
    }

    fn text(&mut self, conn: u64, text: &str) -> Vec<Effect> {
        let step = self.engine.on_text(ConnId(conn), text);
        self.run(step).unwrap()
    }

    fn close(&mut self, conn: u64) -> Vec<Effect> {
        let step = self.engine.on_close(ConnId(conn));
        self.run(step).unwrap()
    }

    /// Open `conn` for `name` and say hello.
    fn join(&mut self, conn: u64, name: &str) -> (SessionToken, Vec<Effect>) {
        let token = token(name);
        self.open(conn, &token);
        let effects = self.send(conn, &hello(name));
        (token, effects)
    }

    fn pick(&mut self, conn: u64, color: Color) -> Vec<Effect> {
        self.send(conn, &PeerMessage::PickColor { color: Some(color) })
    }

    fn saves(&mut self) -> usize {
        std::mem::take(&mut self.host.saved).len()
    }
}

fn token(name: &str) -> SessionToken {
    SessionToken::new(format!("secret-{name}-0123456789"))
}

fn hello(nickname: &str) -> PeerMessage {
    PeerMessage::Hello {
        nickname: nickname.into(),
    }
}

/// The messages `conn` receives, in order, from direct sends and broadcasts.
fn received(effects: &[Effect], conn: u64) -> Vec<&HostMessage> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Send { to, message } if *to == ConnId(conn) => Some(message),
            Effect::Broadcast { to, message } if to.contains(&ConnId(conn)) => Some(message),
            _ => None,
        })
        .collect()
}

fn closes(effects: &[Effect]) -> Vec<(u64, &'static str)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Close { conn, reason } => Some((conn.0, *reason)),
            _ => None,
        })
        .collect()
}

fn error_text(message: &HostMessage) -> (&RejectCode, &str) {
    match message {
        HostMessage::Error { code, message } => (code, message),
        other => panic!("expected an error, got {other:?}"),
    }
}

fn welcome_id(message: &HostMessage) -> String {
    match message {
        HostMessage::Welcome {
            your_peer_id,
            rejected: None,
            ..
        } => your_peer_id.as_str().to_owned(),
        other => panic!("expected a welcome, got {other:?}"),
    }
}

#[test]
fn hello_welcomes_the_player_and_tells_the_others() {
    let mut room = Room::new();
    let (_, effects) = room.join(1, "Red");
    let [welcome] = received(&effects, 1)[..] else {
        panic!("one welcome: {effects:?}");
    };
    assert!(!welcome_id(welcome).is_empty());
    assert_eq!(effects.len(), 1, "nobody else to tell");
    assert_eq!(room.saves(), 1, "a new member is saved");

    let (_, effects) = room.join(2, "Blue");
    let [HostMessage::Welcome { lobby, .. }] = received(&effects, 2)[..] else {
        panic!("one welcome: {effects:?}");
    };
    assert_eq!(lobby.participants.len(), 2);
    assert!(lobby.participants.iter().all(|p| p.connected));
    // The first player hears of the second; the second isn't told twice.
    let [HostMessage::LobbyUpdate { lobby }] = received(&effects, 1)[..] else {
        panic!("a roster update: {effects:?}");
    };
    assert_eq!(lobby.participants.len(), 2);
    assert!(matches!(&effects[1], Effect::Broadcast { to, .. } if to == &[ConnId(1)]));
}

#[test]
fn a_step_changes_nothing_until_it_runs() {
    let mut room = Room::new();
    room.join(1, "Red");
    let step = room.engine.on_text(
        ConnId(1),
        &serde_json::to_string(&PeerMessage::PickColor {
            color: Some(Color::Red),
        })
        .unwrap(),
    );
    assert!(step
        .persist()
        .is_some_and(|next| next.seats[Color::Red].is_some()));
    assert!(room.engine.room().seats[Color::Red].is_none());
    room.run(step).unwrap();
    assert!(room.engine.room().seats[Color::Red].is_some());
}

#[test]
fn a_returning_player_resumes_their_seat_without_a_save() {
    let mut room = Room::new();
    let (red, effects) = room.join(1, "Red");
    let id = welcome_id(received(&effects, 1)[0]);
    room.join(2, "Blue");
    room.pick(1, Color::Red);
    room.saves();

    // Red drops: the seat is kept (nothing to save) and shown offline.
    let effects = room.close(1);
    assert_eq!(room.saves(), 0);
    let [HostMessage::LobbyUpdate { lobby }] = received(&effects, 2)[..] else {
        panic!("a roster update: {effects:?}");
    };
    let seat = lobby
        .participants
        .iter()
        .find(|p| p.seated_color == Some(Color::Red));
    assert!(seat.is_some_and(|p| !p.connected));

    // Back with the same cookie: same player, same seat, no new member.
    room.open(3, &red);
    let effects = room.send(3, &hello("Another name"));
    assert_eq!(room.saves(), 0);
    let [HostMessage::Welcome {
        your_peer_id,
        lobby,
        ..
    }] = received(&effects, 3)[..]
    else {
        panic!("one welcome: {effects:?}");
    };
    assert_eq!(your_peer_id.as_str(), id);
    assert_eq!(lobby.seats[Color::Red].as_ref().unwrap().nickname, "Red");
}

#[test]
fn a_second_tab_takes_over_and_the_first_is_closed() {
    let mut room = Room::new();
    let (red, effects) = room.join(1, "Red");
    let id = welcome_id(received(&effects, 1)[0]);
    room.join(2, "Blue");
    room.saves();

    room.open(3, &red);
    let effects = room.send(3, &hello("Red"));
    assert_eq!(closes(&effects), [(1, OPENED_ELSEWHERE)]);
    assert!(matches!(effects[0], Effect::Close { .. }), "closed first");
    assert_eq!(welcome_id(received(&effects, 3)[0]), id);
    assert!(received(&effects, 1).is_empty());
    assert_eq!(received(&effects, 2).len(), 1);
    assert_eq!(room.engine.connection(ConnId(1)), None);

    // The old socket finishing its close changes nothing: Red is still
    // here, connected through the new tab.
    let effects = room.close(1);
    assert!(effects.is_empty());
    assert_eq!(room.saves(), 0);
    let effects = room.send(2, &PeerMessage::RequestSnapshot);
    let [HostMessage::LobbyUpdate { lobby }] = received(&effects, 2)[..] else {
        panic!("a snapshot: {effects:?}");
    };
    assert!(lobby.participants.iter().all(|p| p.connected));
}

#[test]
fn a_refused_hello_closes_the_socket_and_ignores_a_retry() {
    let mut room = Room::new();
    let ghost = token("ghost");
    room.open(1, &ghost);
    let effects = room.send(1, &hello("   "));
    let [HostMessage::Welcome {
        rejected: Some(RejectCode::InvalidMove),
        ..
    }] = received(&effects, 1)[..]
    else {
        panic!("a rejected welcome: {effects:?}");
    };
    assert_eq!(closes(&effects), [(1, "")]);
    assert_eq!(room.saves(), 0);
    assert_eq!(room.engine.connection(ConnId(1)), None);
    // The socket is closing: a retry on it is ignored, not "already joined".
    assert!(room.send(1, &hello("Ghost")).is_empty());
    assert!(room.engine.room().players.is_empty());

    // A full room turns newcomers away the same way.
    for i in 0..MAX_MEMBERS {
        room.join(10 + i as u64, &format!("p{i}"));
    }
    let (_, effects) = room.join(99, "late");
    assert!(matches!(
        received(&effects, 99)[..],
        [HostMessage::Welcome {
            rejected: Some(RejectCode::RoomFull),
            ..
        }]
    ));
    assert_eq!(closes(&effects), [(99, "")]);
}

#[test]
fn bad_frames_are_answered_before_they_reach_the_room() {
    let mut room = Room::new();
    let red = token("red");
    room.open(1, &red);
    let only_error = |effects: Vec<Effect>| {
        let [Effect::Send {
            to: ConnId(1),
            message,
        }] = &effects[..]
        else {
            panic!("one reply to the sender: {effects:?}");
        };
        let (code, text) = error_text(message);
        assert_eq!(code, &RejectCode::InvalidMove);
        text.to_owned()
    };

    let step = room.engine.on_binary(ConnId(1));
    assert_eq!(only_error(room.run(step).unwrap()), "text messages only");
    let oversize = format!("\"{}\"", "x".repeat(MAX_MESSAGE_BYTES));
    assert_eq!(only_error(room.text(1, &oversize)), "message too large");
    assert_eq!(only_error(room.text(1, "{not json")), "invalid message");
    assert_eq!(
        only_error(room.send(1, &PeerMessage::StartGame)),
        "say hello first"
    );
    room.send(1, &hello("Red"));
    assert_eq!(only_error(room.send(1, &hello("Red"))), "already joined");
    assert_eq!(room.saves(), 1, "only the hello changed the room");

    // Sockets the room doesn't know (closed, or never opened) are ignored.
    assert!(room.text(7, "{not json").is_empty());
    assert!(room.send(7, &hello("Nobody")).is_empty());
    let step = room.engine.on_binary(ConnId(7));
    assert!(room.run(step).unwrap().is_empty());
    assert!(room.close(7).is_empty());
}

#[test]
fn changes_go_to_everyone_and_answers_to_the_sender() {
    let mut room = Room::new();
    room.join(1, "Red");
    room.join(2, "Blue");
    room.saves();

    let effects = room.pick(1, Color::Red);
    assert_eq!(room.saves(), 1);
    let [Effect::Broadcast {
        to,
        message: HostMessage::LobbyUpdate { .. },
    }] = &effects[..]
    else {
        panic!("a lobby broadcast: {effects:?}");
    };
    assert_eq!(to, &[ConnId(1), ConnId(2)]);

    // A rejection goes back to the sender alone and changes nothing.
    let effects = room.pick(2, Color::Red);
    assert_eq!(room.saves(), 0);
    let [Effect::Send {
        to: ConnId(2),
        message,
    }] = &effects[..]
    else {
        panic!("a reply to the sender: {effects:?}");
    };
    assert_eq!(error_text(message).0, &RejectCode::ColorTaken);

    let effects = room.send(2, &PeerMessage::RequestSnapshot);
    assert_eq!(room.saves(), 0);
    assert!(matches!(
        &effects[..],
        [Effect::Send {
            to: ConnId(2),
            message: HostMessage::LobbyUpdate { .. }
        }]
    ));

    room.pick(2, Color::Blue);
    let effects = room.send(2, &PeerMessage::StartGame);
    let [Effect::Broadcast {
        to,
        message: HostMessage::GameStarted { snapshot },
    }] = &effects[..]
    else {
        panic!("a game start for everyone: {effects:?}");
    };
    assert_eq!(to, &[ConnId(1), ConnId(2)]);
    assert_eq!(snapshot.playing_colors.len(), 2);
    let saved = room.host.saved.last().unwrap();
    let game = tantrix_game::Game::from_data(saved.game.clone().unwrap());
    assert_eq!(game.seed(), None, "online deals keep no seed");

    // A newcomer gets the running game; the players hear of them without
    // being sent back to the game-start screen.
    let (_, effects) = room.join(3, "Watcher");
    assert!(matches!(
        received(&effects, 3)[..],
        [HostMessage::Welcome { .. }, HostMessage::GameStarted { .. }]
    ));
    assert!(matches!(
        received(&effects, 1)[..],
        [HostMessage::StateUpdate {
            last_move: None,
            ..
        }]
    ));
}

#[test]
fn disconnects_keep_seats_and_let_watchers_go() {
    let mut room = Room::new();
    let (red, _) = room.join(1, "Red");
    let (watcher, _) = room.join(2, "Watcher");
    room.join(3, "Blue");
    room.pick(1, Color::Red);
    room.saves();

    // A watcher's disconnect removes them from the room.
    let effects = room.close(2);
    assert_eq!(room.saves(), 1);
    assert!(room.engine.room().player(&watcher).is_none());
    let [Effect::Broadcast { to, .. }] = &effects[..] else {
        panic!("a roster update: {effects:?}");
    };
    assert_eq!(to, &[ConnId(1), ConnId(3)]);

    // A seated player's keeps the seat, shown offline.
    let effects = room.close(1);
    assert_eq!(room.saves(), 0);
    assert_eq!(room.engine.room().color_of(&red), Some(Color::Red));
    let [HostMessage::LobbyUpdate { lobby }] = received(&effects, 3)[..] else {
        panic!("a roster update: {effects:?}");
    };
    assert!(lobby
        .participants
        .iter()
        .any(|p| p.seated_color == Some(Color::Red) && !p.connected));

    // A socket that never said hello leaves no trace.
    room.open(4, &token("shy"));
    assert!(room.close(4).is_empty());
    assert_eq!(room.saves(), 0);
}

#[test]
fn a_failed_save_commits_nothing_and_closes_every_socket() {
    let mut room = Room::new();
    let (red, _) = room.join(1, "Red");
    room.join(2, "Blue");
    room.pick(1, Color::Red);
    room.open(3, &token("lurker"));
    let before = serde_json::to_value(room.engine.room()).unwrap();

    room.host.fail_saves = true;
    let step = room.engine.on_text(
        ConnId(2),
        &serde_json::to_string(&PeerMessage::PickColor {
            color: Some(Color::Blue),
        })
        .unwrap(),
    );
    let (error, delivered) = room.run_failing(step);
    assert_eq!(error, "storage is down");
    // Nobody hears of the change storage doesn't hold...
    assert_eq!(
        closes(&delivered),
        [
            (1, STORAGE_UNAVAILABLE),
            (2, STORAGE_UNAVAILABLE),
            (3, STORAGE_UNAVAILABLE)
        ]
    );
    assert_eq!(delivered.len(), 3, "{delivered:?}");
    // ...and the room is still what was saved last.
    assert_eq!(serde_json::to_value(room.engine.room()).unwrap(), before);
    assert_eq!(room.engine.room().color_of(&red), Some(Color::Red));
    assert_eq!(room.engine.connections().count(), 0);
}

#[test]
fn a_new_member_is_not_welcomed_if_their_arrival_cannot_be_saved() {
    let mut room = Room::new();
    room.host.fail_saves = true;
    let newcomer = token("newcomer");
    room.open(1, &newcomer);
    let step = room
        .engine
        .on_text(ConnId(1), &serde_json::to_string(&hello("New")).unwrap());
    let (_, delivered) = room.run_failing(step);
    assert_eq!(closes(&delivered), [(1, STORAGE_UNAVAILABLE)]);
    assert_eq!(delivered.len(), 1, "no welcome: {delivered:?}");
    assert!(room.engine.room().player(&newcomer).is_none());
}

#[test]
fn a_socket_that_cannot_keep_up_is_closed_and_leaves() {
    let mut room = Room::new();
    let (red, _) = room.join(1, "Red");
    let (slow, _) = room.join(2, "Slow");
    room.pick(1, Color::Red);
    room.saves();

    room.host.unreachable.insert(ConnId(2));
    let effects = room.pick(1, Color::Blue);
    // The change, then the slow socket's close and its departure.
    assert_eq!(closes(&effects), [(2, TOO_SLOW)]);
    assert_eq!(room.saves(), 2);
    assert!(room.engine.room().player(&slow).is_none());
    assert_eq!(room.engine.room().color_of(&red), Some(Color::Blue));
    let [first, second] = &received(&effects, 1)[..] else {
        panic!("the change and a roster update: {effects:?}");
    };
    assert!(matches!(first, HostMessage::LobbyUpdate { lobby } if lobby.participants.len() == 2));
    assert!(matches!(second, HostMessage::LobbyUpdate { lobby } if lobby.participants.len() == 1));
    assert_eq!(room.engine.connection(ConnId(2)), None);
}

#[test]
fn loading_lets_go_of_watchers_whose_sockets_died_with_the_server() {
    let mut room = Room::new();
    let (red, _) = room.join(1, "Red");
    let (watcher, _) = room.join(2, "Watcher");
    room.pick(1, Color::Red);
    let stored = room.engine.room().clone();

    // After a restart no socket is open: the seat is kept for Red.
    let restarted = Room::loaded(stored.clone(), []);
    let step = restarted.engine.on_load();
    assert!(step.effects().is_empty());
    let settled = step.persist().expect("the watcher leaves");
    assert!(settled.player(&watcher).is_none());
    assert_eq!(settled.color_of(&red), Some(Color::Red));

    // Woken with the watcher's socket still open (hibernation), nothing
    // changes.
    let woken = Room::loaded(
        stored,
        [(
            ConnId(2),
            Connection {
                token: watcher,
                joined: true,
            },
        )],
    );
    let step = woken.engine.on_load();
    assert!(step.persist().is_none());
    assert!(step.effects().is_empty());
}
