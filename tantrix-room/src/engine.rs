//! One room's orchestration without I/O, shared by both servers.
//!
//! A [`RoomEngine`] holds a room's [`RoomData`] and the sockets open on it.
//! Its adapter (the Durable Object, or the native room actor) turns socket
//! events into calls — [`on_open`](RoomEngine::on_open),
//! [`on_text`](RoomEngine::on_text), [`on_binary`](RoomEngine::on_binary),
//! [`on_close`](RoomEngine::on_close) — and owns only what the engine can't:
//! sockets, keepalives and storage.
//!
//! Each call returns a [`Step`] and changes nothing yet. A step may carry a
//! new room to persist ([`Step::persist`]) and a list of [`Effect`]s: sends,
//! broadcasts and closes. [`RoomEngine::run`] carries it out through a
//! [`RoomHost`], in this order, the same on both servers:
//!
//! 1. If the room changed, the host saves it. Only once the save succeeded
//!    is the step committed (room and sockets), and only then are its
//!    effects delivered, so nobody hears of a change storage doesn't hold.
//! 2. If the save fails, nothing is committed: every socket is closed with
//!    [`STORAGE_UNAVAILABLE`] and the error is returned. The adapter then
//!    drops the engine and loads the room from storage again next time.
//! 3. A socket the host could not hand a message to (its queue is full, or
//!    it is gone) is closed with [`TOO_SLOW`] and leaves the room as a
//!    disconnect would, and the others hear the new roster.
//!
//! A socket must say `Hello` before anything else. A refused `Hello` (full
//! room, bad nickname) gets a rejected `Welcome` and the socket is closed;
//! the engine forgets it, so whatever else it sends is ignored and a retry
//! needs a new socket. Frames from closed or unknown sockets are ignored.
//!
//! Abuse limits, the same on both servers:
//!
//! - A socket may send [`MESSAGE_BURST`] frames at once, then
//!   [`MESSAGES_PER_SECOND`] on average. One that sends faster is closed
//!   with [`TOO_MANY_MESSAGES`] and leaves the room as a disconnect would.
//! - A room keeps at most [`MAX_CONNECTIONS`] sockets. Members are capped,
//!   and each has one joined socket, so the room is full of sockets that
//!   never said `Hello`: the oldest of those is closed with
//!   [`TOO_MANY_CONNECTIONS`] to make room for the new one.

use std::collections::{BTreeMap, HashSet};
use std::future::Future;

use serde::{Deserialize, Serialize};
use tantrix_proto::{HostMessage, PeerId, PeerMessage, RejectCode};

use crate::room_logic::{RoomData, MAX_MEMBERS};
use crate::session_cookie::SessionToken;

/// Larger text frames are refused before they are parsed.
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024;
/// Transports drop frames larger than this before they reach the engine,
/// which still answers those up to it with an error.
pub const MAX_FRAME_BYTES: usize = 8 * MAX_MESSAGE_BYTES;

/// Frames a socket may send at once…
pub const MESSAGE_BURST: u64 = 20;
/// …and then on average per second. Players send a few per turn.
pub const MESSAGES_PER_SECOND: u64 = 2;
/// Sockets a room keeps open: one joined socket per member, and as many
/// again that have not said `Hello` yet.
pub const MAX_CONNECTIONS: usize = 2 * MAX_MEMBERS;

/// Close reason for an older socket of a player who opened the room again.
pub use tantrix_proto::OPENED_ELSEWHERE;
/// Close reason for every socket when the room can't be saved or loaded.
pub const STORAGE_UNAVAILABLE: &str = "Room storage is unavailable.";
/// Close reason for a socket the server could not deliver a message to.
pub const TOO_SLOW: &str = "Connection is too slow.";
/// Close reason for a socket that sends faster than its allowance.
pub const TOO_MANY_MESSAGES: &str = "Too many messages.";
/// Close reason for a socket the room had no room for.
pub const TOO_MANY_CONNECTIONS: &str = "Too many connections to this room.";

/// An open socket, numbered by its adapter. Unique among a room's sockets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConnId(pub u64);

/// What the engine knows about an open socket. The Durable Object keeps it
/// on the socket's attachment, so it survives hibernation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    /// From the handshake's session cookie.
    pub token: SessionToken,
    /// The room accepted this socket's `Hello`.
    pub joined: bool,
}

/// Something the adapter does with its sockets, in order.
#[derive(Debug, Clone)]
pub enum Effect {
    /// Send `message` to one socket.
    Send {
        /// The receiving socket.
        to: ConnId,
        /// Sent as one text frame.
        message: HostMessage,
    },
    /// Send the same `message` to each of `to` (serialize it once).
    Broadcast {
        /// The receiving sockets, in order.
        to: Vec<ConnId>,
        /// Sent as one text frame to each.
        message: HostMessage,
    },
    /// Close the socket normally (code 1000) with `reason`, after what was
    /// already sent to it. The engine has forgotten it.
    Close {
        /// The socket to close.
        conn: ConnId,
        /// Text the client may show: one of this module's close reasons, or
        /// empty after a rejected `Welcome`.
        reason: &'static str,
    },
}

type Connections = BTreeMap<ConnId, Connection>;

/// How many frames a socket may still send: a token bucket, in thousandths
/// of a frame, refilled at [`MESSAGES_PER_SECOND`] up to [`MESSAGE_BURST`].
/// Kept in memory only: a Durable Object that hibernated, or a restarted
/// server, gives its sockets a full allowance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Allowance {
    millis: u64,
    /// When `millis` was last worked out, in milliseconds.
    at: u64,
}

impl Allowance {
    const FULL: u64 = MESSAGE_BURST * 1000;

    fn full(now: u64) -> Self {
        Self {
            millis: Self::FULL,
            at: now,
        }
    }

    /// The allowance after one more frame at `now`, or `None` when there is
    /// none left. A clock that went back refills nothing.
    fn spend(self, now: u64) -> Option<Self> {
        let refill = now
            .saturating_sub(self.at)
            .saturating_mul(MESSAGES_PER_SECOND);
        let millis = self.millis.saturating_add(refill).min(Self::FULL);
        Some(Self {
            millis: millis.checked_sub(1000)?,
            at: now.max(self.at),
        })
    }
}

/// What an event leads to. Nothing changes until it is
/// [committed](RoomEngine::commit), which [`RoomEngine::run`] does once the
/// new room, if any, is saved.
#[must_use = "a step changes nothing until it is run or committed"]
#[derive(Debug, Default)]
pub struct Step {
    room: Option<RoomData>,
    connections: Option<Connections>,
    effects: Vec<Effect>,
    /// What is left of the sender's allowance after this frame.
    spent: Option<(ConnId, Allowance)>,
}

impl Step {
    /// The changed room, to save before committing the step; `None` when
    /// the room did not change.
    pub fn persist(&self) -> Option<&RoomData> {
        self.room.as_ref()
    }

    /// The sends, broadcasts and closes to carry out, in order, once the
    /// step is committed.
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }

    fn reply(to: ConnId, message: HostMessage) -> Self {
        Self {
            effects: vec![Effect::Send { to, message }],
            ..Self::default()
        }
    }

    fn error(to: ConnId, message: &str) -> Self {
        Self::reply(
            to,
            HostMessage::Error {
                code: RejectCode::InvalidMove,
                message: message.into(),
            },
        )
    }
}

/// The I/O a server does for [`RoomEngine::run`].
pub trait RoomHost {
    /// Why a save failed.
    type Error;

    /// Durably store the room.
    fn save(&mut self, room: &RoomData) -> impl Future<Output = Result<(), Self::Error>>;

    /// Carry out `effects` in order, and return the sockets that could not
    /// take a message, including any the host no longer has.
    fn deliver(&mut self, effects: Vec<Effect>) -> Vec<ConnId>;
}

/// A room and its open sockets. See the [module docs](self).
#[derive(Debug, Clone, Default)]
pub struct RoomEngine {
    room: RoomData,
    connections: Connections,
    /// Of the sockets in `connections` that sent something.
    allowances: BTreeMap<ConnId, Allowance>,
}

impl RoomEngine {
    /// A room as storage holds it, with the sockets still open on it: none
    /// after a process restart; on the Worker, those that outlived the
    /// object's hibernation. Run [`Self::on_load`] next.
    pub fn new(
        room: RoomData,
        connections: impl IntoIterator<Item = (ConnId, Connection)>,
    ) -> Self {
        Self {
            room,
            connections: connections.into_iter().collect(),
            allowances: BTreeMap::new(),
        }
    }

    /// The room as last committed.
    pub fn room(&self) -> &RoomData {
        &self.room
    }

    /// What the engine knows about socket `conn`; `None` for a closed or
    /// unknown socket.
    pub fn connection(&self, conn: ConnId) -> Option<&Connection> {
        self.connections.get(&conn)
    }

    /// The open sockets the engine knows, in [`ConnId`] order.
    pub fn connections(&self) -> impl Iterator<Item = (ConnId, &Connection)> {
        self.connections.iter().map(|(conn, c)| (*conn, c))
    }

    /// Settle a room just loaded from storage. A restart closes sockets
    /// without telling the room, so members without a seat who are not
    /// connected any more leave, as their disconnect would have made them;
    /// seated players keep their seats for when they come back.
    pub fn on_load(&self) -> Step {
        let connected = joined_tokens(&self.connections);
        let mut room = self.room.clone();
        let mut changed = false;
        for player in &self.room.players {
            if !connected.contains(&player.token) {
                changed |= room.disconnect(&player.token);
            }
        }
        if !changed {
            return Step::default();
        }
        let effects = broadcast(
            joined_ids(&self.connections, None),
            room.roster_message(&connected),
        );
        Step {
            room: Some(room),
            effects,
            ..Step::default()
        }
    }

    /// A socket was accepted, with the session token of its handshake. It
    /// hears nothing until it says `Hello`. A room with [`MAX_CONNECTIONS`]
    /// sockets first closes its oldest socket that has not said `Hello`, or
    /// closes this one if there is none.
    pub fn on_open(&self, conn: ConnId, token: SessionToken) -> Step {
        let mut connections = self.connections.clone();
        let mut effects = Vec::new();
        if connections.len() >= MAX_CONNECTIONS {
            // Socket numbers grow, so the first waiting socket is the oldest.
            let Some(oldest) = connections
                .iter()
                .find(|(_, c)| !c.joined)
                .map(|(id, _)| *id)
            else {
                return Step {
                    effects: vec![Effect::Close {
                        conn,
                        reason: TOO_MANY_CONNECTIONS,
                    }],
                    ..Step::default()
                };
            };
            connections.remove(&oldest);
            effects.push(Effect::Close {
                conn: oldest,
                reason: TOO_MANY_CONNECTIONS,
            });
        }
        connections.insert(
            conn,
            Connection {
                token,
                joined: false,
            },
        );
        Step {
            connections: Some(connections),
            effects,
            ..Step::default()
        }
    }

    /// A text frame arrived at `now`, in milliseconds. A clock that goes
    /// back refills no allowance until it catches up.
    pub fn on_text(&self, conn: ConnId, text: &str, now: u64) -> Step {
        let Some(connection) = self.connections.get(&conn) else {
            return Step::default();
        };
        let Some(allowance) = self.spend(conn, now) else {
            return self.drop_connections(&[conn], Some(TOO_MANY_MESSAGES));
        };
        let step = if text.len() > MAX_MESSAGE_BYTES {
            Step::error(conn, "message too large")
        } else {
            match tantrix_proto::decode::<PeerMessage>(text.as_bytes()) {
                Err(_) => Step::error(conn, "invalid message"),
                Ok(PeerMessage::Hello { .. }) if connection.joined => {
                    Step::error(conn, "already joined")
                }
                Ok(PeerMessage::Hello { nickname }) => {
                    self.hello(conn, &connection.token, nickname)
                }
                Ok(_) if !connection.joined => Step::error(conn, "say hello first"),
                Ok(intent) => self.intent(conn, &connection.token, intent),
            }
        };
        Step {
            spent: Some((conn, allowance)),
            ..step
        }
    }

    /// A binary frame arrived at `now`: the protocol is text only.
    pub fn on_binary(&self, conn: ConnId, now: u64) -> Step {
        if !self.connections.contains_key(&conn) {
            return Step::default();
        }
        let Some(allowance) = self.spend(conn, now) else {
            return self.drop_connections(&[conn], Some(TOO_MANY_MESSAGES));
        };
        Step {
            spent: Some((conn, allowance)),
            ..Step::error(conn, "text messages only")
        }
    }

    /// `conn`'s allowance after one more frame at `now`, or `None` when it
    /// has none left: the caller closes it.
    fn spend(&self, conn: ConnId, now: u64) -> Option<Allowance> {
        self.allowances
            .get(&conn)
            .copied()
            .unwrap_or_else(|| Allowance::full(now))
            .spend(now)
    }

    /// A socket closed (or errored). A seated player keeps the seat; anyone
    /// else leaves the room. The others hear who is still connected.
    pub fn on_close(&self, conn: ConnId) -> Step {
        self.drop_connections(&[conn], None)
    }

    /// The host could not hand these sockets a message: close them and
    /// treat them as disconnected. [`Self::run`] does this itself.
    pub fn on_unreachable(&self, conns: &[ConnId]) -> Step {
        self.drop_connections(conns, Some(TOO_SLOW))
    }

    /// Apply `step` and return its effects. Only call this once its room,
    /// if any, is saved — or let [`Self::run`] do both.
    pub fn commit(&mut self, step: Step) -> Vec<Effect> {
        if let Some(room) = step.room {
            self.room = room;
        }
        if let Some(connections) = step.connections {
            self.connections = connections;
            let connections = &self.connections;
            self.allowances
                .retain(|conn, _| connections.contains_key(conn));
        }
        if let Some((conn, allowance)) = step.spent {
            if self.connections.contains_key(&conn) {
                self.allowances.insert(conn, allowance);
            }
        }
        step.effects
    }

    /// Carry out `step` through `host`: save, commit, deliver, then drop
    /// any socket the host could not deliver to (which may change the room
    /// again). On a failed save nothing is committed, every socket is closed
    /// with [`STORAGE_UNAVAILABLE`], and the error is returned: drop this
    /// engine and reload the room before serving it again.
    pub async fn run<H: RoomHost>(&mut self, host: &mut H, mut step: Step) -> Result<(), H::Error> {
        loop {
            if let Some(room) = step.persist() {
                if let Err(error) = host.save(room).await {
                    let sockets = std::mem::take(&mut self.connections);
                    host.deliver(storage_unavailable(sockets.into_keys()));
                    return Err(error);
                }
            }
            let effects = self.commit(step);
            let unreachable = host.deliver(effects);
            if unreachable.is_empty() {
                return Ok(());
            }
            step = self.on_unreachable(&unreachable);
        }
    }

    /// Register or resume the player behind `token`. The same cookie is the
    /// same player: a reload, reconnect or second tab takes over, and any
    /// older socket of theirs is closed.
    fn hello(&self, conn: ConnId, token: &SessionToken, nickname: String) -> Step {
        let (joined_room, peer_id) = match self.room.player(token) {
            Some(existing) => (None, existing.id.clone()),
            None => {
                let mut room = self.room.clone();
                let new_id = PeerId::new(uuid::Uuid::new_v4().to_string());
                match room.hello(token.clone(), nickname, new_id) {
                    Ok(peer_id) => (Some(room), peer_id),
                    Err(code) => {
                        let mut connections = self.connections.clone();
                        connections.remove(&conn);
                        return Step {
                            connections: Some(connections),
                            effects: vec![
                                Effect::Send {
                                    to: conn,
                                    message: rejected_welcome(code),
                                },
                                Effect::Close { conn, reason: "" },
                            ],
                            ..Step::default()
                        };
                    }
                }
            }
        };
        let room = joined_room.as_ref().unwrap_or(&self.room);

        let mut connections = self.connections.clone();
        let mut effects = Vec::new();
        let older: Vec<ConnId> = connections
            .iter()
            .filter(|(id, c)| **id != conn && c.joined && &c.token == token)
            .map(|(id, _)| *id)
            .collect();
        for old in older {
            connections.remove(&old);
            effects.push(Effect::Close {
                conn: old,
                reason: OPENED_ELSEWHERE,
            });
        }
        if let Some(connection) = connections.get_mut(&conn) {
            connection.joined = true;
        }

        let connected = joined_tokens(&connections);
        effects.push(Effect::Send {
            to: conn,
            message: HostMessage::Welcome {
                your_peer_id: peer_id,
                lobby: room.lobby(&connected),
                rejected: None,
            },
        });
        if room.game.is_some() {
            effects.push(Effect::Send {
                to: conn,
                message: room.current_message(&connected),
            });
        }
        effects.extend(broadcast(
            joined_ids(&connections, Some(conn)),
            room.roster_message(&connected),
        ));
        Step {
            room: joined_room,
            connections: Some(connections),
            effects,
            ..Step::default()
        }
    }

    /// Apply a joined player's intent: a change is saved and broadcast to
    /// everyone; an answer (`RequestSnapshot`) or a rejection goes back to
    /// the sender alone.
    fn intent(&self, conn: ConnId, token: &SessionToken, intent: PeerMessage) -> Step {
        let connected = joined_tokens(&self.connections);
        let mut room = self.room.clone();
        match room.apply(token, intent, &connected) {
            Ok(change) if change.persist => Step {
                effects: broadcast(joined_ids(&self.connections, None), change.message),
                room: Some(room),
                ..Step::default()
            },
            Ok(change) => Step::reply(conn, change.message),
            Err((code, message)) => Step::reply(conn, HostMessage::Error { code, message }),
        }
    }

    /// Forget `gone` sockets, closing them with `close` if given. Players
    /// left with no joined socket disconnect from the room, and the joined
    /// sockets that remain hear the new roster.
    fn drop_connections(&self, gone: &[ConnId], close: Option<&'static str>) -> Step {
        let mut connections = self.connections.clone();
        let mut effects = Vec::new();
        let mut left = Vec::new();
        for &conn in gone {
            let Some(connection) = connections.remove(&conn) else {
                continue;
            };
            if let Some(reason) = close {
                effects.push(Effect::Close { conn, reason });
            }
            if connection.joined {
                left.push(connection.token);
            }
        }
        if connections.len() == self.connections.len() {
            return Step::default();
        }

        let connected = joined_tokens(&connections);
        let mut next = None;
        let mut changed = false;
        for token in left.iter().filter(|token| !connected.contains(*token)) {
            changed |= next
                .get_or_insert_with(|| self.room.clone())
                .disconnect(token);
        }
        let room = next.filter(|_| changed);
        if !left.is_empty() {
            let current = room.as_ref().unwrap_or(&self.room);
            effects.extend(broadcast(
                joined_ids(&connections, None),
                current.roster_message(&connected),
            ));
        }
        Step {
            room,
            connections: Some(connections),
            effects,
            ..Step::default()
        }
    }
}

/// The `Welcome` that turns a socket away: a refused `Hello`, or a room
/// that does not exist. Close the socket after sending it.
pub fn rejected_welcome(code: RejectCode) -> HostMessage {
    HostMessage::Welcome {
        your_peer_id: PeerId::new(""),
        lobby: RoomData::default().lobby(&HashSet::new()),
        rejected: Some(code),
    }
}

/// Close every one of `conns`: the room could not be saved or loaded.
pub fn storage_unavailable(conns: impl IntoIterator<Item = ConnId>) -> Vec<Effect> {
    conns
        .into_iter()
        .map(|conn| Effect::Close {
            conn,
            reason: STORAGE_UNAVAILABLE,
        })
        .collect()
}

fn joined_tokens(connections: &Connections) -> HashSet<SessionToken> {
    connections
        .values()
        .filter(|c| c.joined)
        .map(|c| c.token.clone())
        .collect()
}

fn joined_ids(connections: &Connections, except: Option<ConnId>) -> Vec<ConnId> {
    connections
        .iter()
        .filter(|(id, c)| c.joined && Some(**id) != except)
        .map(|(id, _)| *id)
        .collect()
}

/// A broadcast to `to`, or nothing when nobody would hear it.
fn broadcast(to: Vec<ConnId>, message: HostMessage) -> Vec<Effect> {
    if to.is_empty() {
        Vec::new()
    } else {
        vec![Effect::Broadcast { to, message }]
    }
}
