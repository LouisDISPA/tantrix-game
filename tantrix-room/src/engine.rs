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

use std::collections::{BTreeMap, HashSet};
use std::future::Future;

use serde::{Deserialize, Serialize};
use tantrix_proto::{HostMessage, PeerId, PeerMessage, RejectCode};

use crate::room_logic::RoomData;
use crate::session_cookie::SessionToken;

/// Larger text frames are refused before they are parsed.
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024;

/// Close reason for an older socket of a player who opened the room again.
pub const OPENED_ELSEWHERE: &str = "This room was opened in another tab.";
/// Close reason for every socket when the room can't be saved or loaded.
pub const STORAGE_UNAVAILABLE: &str = "Room storage is unavailable.";
/// Close reason for a socket the server could not deliver a message to.
pub const TOO_SLOW: &str = "Connection is too slow.";

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
    Send { to: ConnId, message: HostMessage },
    /// Send the same `message` to each of `to` (serialize it once).
    Broadcast {
        to: Vec<ConnId>,
        message: HostMessage,
    },
    /// Close the socket normally (code 1000) with `reason`, after what was
    /// already sent to it. The engine has forgotten it.
    Close { conn: ConnId, reason: &'static str },
}

type Connections = BTreeMap<ConnId, Connection>;

/// What an event leads to. Nothing changes until it is
/// [committed](RoomEngine::commit), which [`RoomEngine::run`] does once the
/// new room, if any, is saved.
#[must_use = "a step changes nothing until it is run or committed"]
#[derive(Debug, Default)]
pub struct Step {
    room: Option<RoomData>,
    connections: Option<Connections>,
    effects: Vec<Effect>,
}

impl Step {
    /// The changed room, to save before committing the step; `None` when
    /// the room did not change.
    pub fn persist(&self) -> Option<&RoomData> {
        self.room.as_ref()
    }

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
        }
    }

    pub fn room(&self) -> &RoomData {
        &self.room
    }

    pub fn connection(&self, conn: ConnId) -> Option<&Connection> {
        self.connections.get(&conn)
    }

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
            connections: None,
            effects,
        }
    }

    /// A socket was accepted, with the session token of its handshake. It
    /// hears nothing until it says `Hello`.
    pub fn on_open(&self, conn: ConnId, token: SessionToken) -> Step {
        let mut connections = self.connections.clone();
        connections.insert(
            conn,
            Connection {
                token,
                joined: false,
            },
        );
        Step {
            connections: Some(connections),
            ..Step::default()
        }
    }

    /// A text frame arrived.
    pub fn on_text(&self, conn: ConnId, text: &str) -> Step {
        let Some(connection) = self.connections.get(&conn) else {
            return Step::default();
        };
        if text.len() > MAX_MESSAGE_BYTES {
            return Step::error(conn, "message too large");
        }
        let Ok(intent) = tantrix_proto::decode::<PeerMessage>(text.as_bytes()) else {
            return Step::error(conn, "invalid message");
        };
        match intent {
            PeerMessage::Hello { .. } if connection.joined => Step::error(conn, "already joined"),
            PeerMessage::Hello { nickname } => self.hello(conn, &connection.token, nickname),
            _ if !connection.joined => Step::error(conn, "say hello first"),
            intent => self.intent(conn, &connection.token, intent),
        }
    }

    /// A binary frame arrived: the protocol is text only.
    pub fn on_binary(&self, conn: ConnId) -> Step {
        if self.connections.contains_key(&conn) {
            Step::error(conn, "text messages only")
        } else {
            Step::default()
        }
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
                            room: None,
                            connections: Some(connections),
                            effects: vec![
                                Effect::Send {
                                    to: conn,
                                    message: rejected_welcome(code),
                                },
                                Effect::Close { conn, reason: "" },
                            ],
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
                connections: None,
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
