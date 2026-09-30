//! Native room WebSockets. One task serializes all changes within each room.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

use axum::extract::ws::{close_code, CloseFrame, Message, WebSocket};
use futures::{SinkExt, StreamExt};
use tantrix_net::normalize_room_code;
use tantrix_proto::{HostMessage, PeerId, PeerMessage, RejectCode, KEEPALIVE_PING, KEEPALIVE_PONG};
use tantrix_room::room_logic::RoomData;
use tantrix_room::session_cookie::SessionToken;
use tokio::sync::{mpsc, oneshot, watch, Mutex};

use super::store::{RoomStore, StoreResult};

const MAX_MESSAGE_BYTES: usize = 8 * 1024;
const OUTGOING_CAPACITY: usize = 64;
const COMMAND_CAPACITY: usize = 64;

type ConnectionId = uuid::Uuid;

pub struct RoomManager {
    rooms: Mutex<HashMap<String, Weak<RoomHandle>>>,
    store: RoomStore,
}

pub enum OpenRoom {
    Ready(Arc<RoomHandle>),
    Missing,
    Collision,
}

impl RoomManager {
    pub fn new(store: RoomStore) -> Self {
        Self {
            rooms: Mutex::new(HashMap::new()),
            store,
        }
    }

    pub async fn open(&self, code: &str, create: bool) -> StoreResult<OpenRoom> {
        let mut rooms = self.rooms.lock().await;
        rooms.retain(|_, handle| handle.strong_count() > 0);
        if let Some(handle) = rooms.get(code).and_then(Weak::upgrade) {
            if !handle.tx.is_closed() {
                return Ok(if create {
                    OpenRoom::Collision
                } else {
                    OpenRoom::Ready(handle)
                });
            }
        }

        let mut room = if create {
            if !self.store.create(code).await? {
                return Ok(OpenRoom::Collision);
            }
            RoomData::default()
        } else {
            let Some(room) = self.store.load(code).await? else {
                return Ok(OpenRoom::Missing);
            };
            room
        };

        // A process restart loses every socket. Retain seats for reconnects,
        // and remove members who had no seat, as a normal disconnect would.
        let tokens: Vec<_> = room.players.iter().map(|p| p.token.clone()).collect();
        let mut changed = false;
        for token in &tokens {
            changed |= room.disconnect(token);
        }
        if changed {
            self.store.save(code, &room).await?;
        }

        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let handle = Arc::new(RoomHandle { tx });
        rooms.insert(code.to_owned(), Arc::downgrade(&handle));
        tokio::spawn(RoomActor::new(code.to_owned(), room, self.store.clone()).run(rx));
        Ok(OpenRoom::Ready(handle))
    }
}

pub struct RoomHandle {
    tx: mpsc::Sender<RoomCommand>,
}

impl RoomHandle {
    async fn send(&self, command: RoomCommand) -> bool {
        self.tx.send(command).await.is_ok()
    }
}

enum RoomCommand {
    Hello {
        id: ConnectionId,
        token: SessionToken,
        nickname: String,
        outgoing: mpsc::Sender<Message>,
        close: watch::Sender<Option<String>>,
    },
    Intent {
        id: ConnectionId,
        message: PeerMessage,
    },
    Disconnect {
        id: ConnectionId,
        done: oneshot::Sender<()>,
    },
}

struct Connection {
    token: SessionToken,
    outgoing: mpsc::Sender<Message>,
    close: watch::Sender<Option<String>>,
}

struct RoomActor {
    code: String,
    data: RoomData,
    connections: HashMap<ConnectionId, Connection>,
    store: RoomStore,
}

impl RoomActor {
    fn new(code: String, data: RoomData, store: RoomStore) -> Self {
        Self {
            code,
            data,
            connections: HashMap::new(),
            store,
        }
    }

    async fn run(mut self, mut commands: mpsc::Receiver<RoomCommand>) {
        while let Some(command) = commands.recv().await {
            let result = match command {
                RoomCommand::Hello {
                    id,
                    token,
                    nickname,
                    outgoing,
                    close,
                } => self.hello(id, token, nickname, outgoing, close).await,
                RoomCommand::Intent { id, message } => self.intent(id, message).await,
                RoomCommand::Disconnect { id, done } => {
                    let result = self.disconnect(id).await;
                    let result = match result {
                        Ok(()) => {
                            self.broadcast(self.data.roster_message(&self.connected()), None)
                                .await
                        }
                        Err(error) => Err(error),
                    };
                    let _ = done.send(());
                    result
                }
            };
            if let Err(error) = result {
                log::error!("room {} stopped after storage error: {error}", self.code);
                for connection in self.connections.values() {
                    let _ = connection
                        .close
                        .send(Some("Room storage is unavailable.".into()));
                }
                break;
            }
        }
    }

    fn connected(&self) -> HashSet<SessionToken> {
        self.connections
            .values()
            .map(|connection| connection.token.clone())
            .collect()
    }

    async fn hello(
        &mut self,
        id: ConnectionId,
        token: SessionToken,
        nickname: String,
        outgoing: mpsc::Sender<Message>,
        close: watch::Sender<Option<String>>,
    ) -> StoreResult<()> {
        if self.connections.contains_key(&id) {
            send(
                &outgoing,
                &HostMessage::Error {
                    code: RejectCode::InvalidMove,
                    message: "already joined".into(),
                },
            );
            return Ok(());
        }
        let mut next = self.data.clone();
        let existed = next.player(&token).is_some();
        let peer_id = match next.hello(
            token.clone(),
            nickname,
            PeerId::new(uuid::Uuid::new_v4().to_string()),
        ) {
            Ok(peer_id) => peer_id,
            Err(code) => {
                reject_welcome(&outgoing, code);
                return Ok(());
            }
        };
        if !existed {
            self.store.save(&self.code, &next).await?;
        }
        self.data = next;

        // The new connection supersedes every older tab with the same cookie.
        let old_ids: Vec<_> = self
            .connections
            .iter()
            .filter_map(|(old_id, connection)| (connection.token == token).then_some(*old_id))
            .collect();
        for old_id in old_ids {
            if let Some(old) = self.connections.remove(&old_id) {
                let _ = old
                    .close
                    .send(Some("This room was opened in another tab.".into()));
            }
        }
        self.connections.insert(
            id,
            Connection {
                token,
                outgoing: outgoing.clone(),
                close,
            },
        );
        let connected = self.connected();
        send(
            &outgoing,
            &HostMessage::Welcome {
                your_peer_id: peer_id,
                lobby: self.data.lobby(&connected),
                rejected: None,
            },
        );
        if self.data.game.is_some() {
            send(&outgoing, &self.data.current_message(&connected));
        }
        self.broadcast(self.data.roster_message(&connected), Some(id))
            .await
    }

    async fn intent(&mut self, id: ConnectionId, message: PeerMessage) -> StoreResult<()> {
        let Some(connection) = self.connections.get(&id) else {
            return Ok(());
        };
        let token = connection.token.clone();
        let outgoing = connection.outgoing.clone();
        let connected = self.connected();
        let mut next = self.data.clone();
        match next.apply(&token, message, &connected) {
            Ok(change) if change.persist => {
                self.store.save(&self.code, &next).await?;
                self.data = next;
                self.broadcast(change.message, None).await?;
            }
            Ok(change) => send(&outgoing, &change.message),
            Err((code, message)) => send(&outgoing, &HostMessage::Error { code, message }),
        }
        Ok(())
    }

    async fn disconnect(&mut self, id: ConnectionId) -> StoreResult<()> {
        let Some(connection) = self.connections.remove(&id) else {
            return Ok(());
        };
        let mut next = self.data.clone();
        if next.disconnect(&connection.token) {
            self.store.save(&self.code, &next).await?;
            self.data = next;
        }
        Ok(())
    }

    async fn broadcast(
        &mut self,
        message: HostMessage,
        except: Option<ConnectionId>,
    ) -> StoreResult<()> {
        let mut pending = Some((message, except));
        while let Some((message, except)) = pending.take() {
            let wire = wire_message(&message);
            let slow: Vec<_> = self
                .connections
                .iter()
                .filter(|(id, _)| Some(**id) != except)
                .filter_map(|(id, connection)| {
                    connection
                        .outgoing
                        .try_send(wire.clone())
                        .is_err()
                        .then_some(*id)
                })
                .collect();
            if slow.is_empty() {
                break;
            }
            for id in slow {
                if let Some(connection) = self.connections.get(&id) {
                    let _ = connection
                        .close
                        .send(Some("Connection is too slow.".into()));
                }
                self.disconnect(id).await?;
            }
            pending = Some((self.data.roster_message(&self.connected()), None));
        }
        Ok(())
    }
}

fn wire_message(message: &HostMessage) -> Message {
    Message::Text(
        serde_json::to_string(message)
            .expect("host message serializes")
            .into(),
    )
}

fn send(outgoing: &mpsc::Sender<Message>, message: &HostMessage) {
    let _ = outgoing.try_send(wire_message(message));
}

fn reject_welcome(outgoing: &mpsc::Sender<Message>, code: RejectCode) {
    send(
        outgoing,
        &HostMessage::Welcome {
            your_peer_id: PeerId::new(""),
            lobby: RoomData::default().lobby(&HashSet::new()),
            rejected: Some(code),
        },
    );
    let _ = outgoing.try_send(Message::Close(Some(CloseFrame {
        code: close_code::NORMAL,
        reason: "".into(),
    })));
}

pub async fn missing_room(mut socket: WebSocket) {
    let message = HostMessage::Welcome {
        your_peer_id: PeerId::new(""),
        lobby: RoomData::default().lobby(&HashSet::new()),
        rejected: Some(RejectCode::RoomNotFound),
    };
    let _ = socket.send(wire_message(&message)).await;
    let _ = socket.close().await;
}

pub async fn socket_session(socket: WebSocket, handle: Arc<RoomHandle>, token: SessionToken) {
    let id = uuid::Uuid::new_v4();
    let (mut writer, mut reader) = socket.split();
    let (outgoing, mut incoming) = mpsc::channel(OUTGOING_CAPACITY);
    let (close, mut closed) = watch::channel::<Option<String>>(None);
    let mut hello_sent = false;

    loop {
        tokio::select! {
            frame = reader.next() => {
                let Some(Ok(frame)) = frame else { break; };
                let text = match frame {
                    Message::Text(text) => text,
                    Message::Close(_) => break,
                    Message::Binary(_) => {
                        send(&outgoing, &HostMessage::Error {
                            code: RejectCode::InvalidMove,
                            message: "text messages only".into(),
                        });
                        continue;
                    }
                    Message::Ping(_) | Message::Pong(_) => continue,
                };
                if text == KEEPALIVE_PING {
                    let _ = outgoing.try_send(Message::Text(KEEPALIVE_PONG.into()));
                    continue;
                }
                if text.len() > MAX_MESSAGE_BYTES {
                    send(&outgoing, &HostMessage::Error {
                        code: RejectCode::InvalidMove,
                        message: "message too large".into(),
                    });
                    continue;
                }
                let Ok(intent) = serde_json::from_str::<PeerMessage>(&text) else {
                    send(&outgoing, &HostMessage::Error {
                        code: RejectCode::InvalidMove,
                        message: "invalid message".into(),
                    });
                    continue;
                };
                let command = match intent {
                    PeerMessage::Hello { nickname } => {
                        if hello_sent {
                            send(&outgoing, &HostMessage::Error {
                                code: RejectCode::InvalidMove,
                                message: "already joined".into(),
                            });
                            continue;
                        }
                        hello_sent = true;
                        RoomCommand::Hello {
                            id,
                            token: token.clone(),
                            nickname,
                            outgoing: outgoing.clone(),
                            close: close.clone(),
                        }
                    }
                    message if !hello_sent => {
                        send(&outgoing, &HostMessage::Error {
                            code: RejectCode::InvalidMove,
                            message: "say hello first".into(),
                        });
                        let _ = message;
                        continue;
                    }
                    message => RoomCommand::Intent { id, message },
                };
                if !handle.send(command).await { break; }
            }
            message = incoming.recv() => {
                let Some(message) = message else { break; };
                let is_close = matches!(message, Message::Close(_));
                if writer.send(message).await.is_err() || is_close { break; }
            }
            changed = closed.changed() => {
                if changed.is_ok() {
                    let reason = closed.borrow().clone().unwrap_or_default();
                    let _ = writer.send(Message::Close(Some(CloseFrame {
                        code: close_code::NORMAL,
                        reason: reason.into(),
                    }))).await;
                }
                break;
            }
            _ = handle.tx.closed() => break,
        }
    }
    let (done, acknowledged) = oneshot::channel();
    if handle.send(RoomCommand::Disconnect { id, done }).await {
        let _ = acknowledged.await;
    }
}

pub fn valid_code(code: &str) -> bool {
    normalize_room_code(code).as_deref() == Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tantrix_game::Color;

    #[tokio::test]
    async fn a_room_reloads_its_seat_after_the_actor_stops() {
        let store = RoomStore::open(":memory:").unwrap();
        let manager = RoomManager::new(store.clone());
        let OpenRoom::Ready(handle) = manager.open("ABCDEF", true).await.unwrap() else {
            panic!("room should be created");
        };
        let token = SessionToken::new("returning-player-123456");
        let id = uuid::Uuid::new_v4();
        let (outgoing, mut messages) = mpsc::channel(8);
        let (close, _closed) = watch::channel(None);
        assert!(
            handle
                .send(RoomCommand::Hello {
                    id,
                    token: token.clone(),
                    nickname: "Player".into(),
                    outgoing,
                    close,
                })
                .await
        );
        assert!(matches!(messages.recv().await, Some(Message::Text(_))));
        assert!(
            handle
                .send(RoomCommand::Intent {
                    id,
                    message: PeerMessage::PickColor {
                        color: Some(Color::Red)
                    },
                })
                .await
        );
        assert!(matches!(messages.recv().await, Some(Message::Text(_))));
        let (done, acknowledged) = oneshot::channel();
        assert!(handle.send(RoomCommand::Disconnect { id, done }).await);
        acknowledged.await.unwrap();
        drop(handle);
        drop(manager);

        let restarted = RoomManager::new(store);
        let OpenRoom::Ready(handle) = restarted.open("ABCDEF", false).await.unwrap() else {
            panic!("room should be restored");
        };
        let (outgoing, mut messages) = mpsc::channel(8);
        let (close, _closed) = watch::channel(None);
        assert!(
            handle
                .send(RoomCommand::Hello {
                    id: uuid::Uuid::new_v4(),
                    token,
                    nickname: "Ignored on reconnect".into(),
                    outgoing,
                    close,
                })
                .await
        );
        let Some(Message::Text(message)) = messages.recv().await else {
            panic!("expected welcome");
        };
        let HostMessage::Welcome { lobby, .. } = serde_json::from_str(&message).unwrap() else {
            panic!("expected welcome");
        };
        assert_eq!(lobby.seats[Color::Red].as_ref().unwrap().nickname, "Player");
    }
}
