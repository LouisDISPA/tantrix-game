//! Native room WebSockets. One actor task per room feeds its sockets'
//! events to a `RoomEngine` (one at a time) and carries out its effects;
//! the room rules and orchestration live in `tantrix_room::engine`.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use axum::extract::ws::{close_code, CloseFrame, Message, WebSocket};
use futures::{SinkExt, StreamExt};
use tantrix_proto::{RejectCode, KEEPALIVE_PING, KEEPALIVE_PONG};
use tantrix_room::engine::{self, ConnId, Effect, RoomEngine, RoomHost};
use tantrix_room::room_logic::RoomData;
use tantrix_room::session_cookie::SessionToken;
use tokio::sync::{mpsc, oneshot, watch, Mutex};

use super::store::{RoomStore, StoreError, StoreResult};

const OUTGOING_CAPACITY: usize = 64;
const COMMAND_CAPACITY: usize = 64;

/// Socket numbers, unique across the process and so within each room.
static NEXT_CONN: AtomicU64 = AtomicU64::new(1);

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

        let room = if create {
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

        // A process restart lost every socket: no actor holds this room.
        let mut engine = RoomEngine::new(room, []);
        let mut sockets = HashMap::new();
        let mut host = Host {
            code,
            store: &self.store,
            sockets: &mut sockets,
        };
        let loaded = engine.on_load();
        engine.run(&mut host, loaded).await?;

        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let handle = Arc::new(RoomHandle { tx });
        rooms.insert(code.to_owned(), Arc::downgrade(&handle));
        tokio::spawn(RoomActor::new(code.to_owned(), engine, self.store.clone()).run(rx));
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

/// One socket event, in the order its socket task saw it.
enum RoomCommand {
    Open {
        conn: ConnId,
        token: SessionToken,
        socket: Socket,
    },
    Text {
        conn: ConnId,
        text: String,
    },
    Binary {
        conn: ConnId,
    },
    Close {
        conn: ConnId,
        done: oneshot::Sender<()>,
    },
}

/// How the actor reaches one socket task.
struct Socket {
    /// Frames to write, in order. Bounded: a full queue is a slow consumer.
    outgoing: mpsc::Sender<Message>,
    /// Close now with this reason, when the queue can't take a close frame.
    close: watch::Sender<Option<String>>,
}

struct RoomActor {
    code: String,
    engine: RoomEngine,
    sockets: HashMap<ConnId, Socket>,
    store: RoomStore,
}

impl RoomActor {
    fn new(code: String, engine: RoomEngine, store: RoomStore) -> Self {
        Self {
            code,
            engine,
            sockets: HashMap::new(),
            store,
        }
    }

    async fn run(mut self, mut commands: mpsc::Receiver<RoomCommand>) {
        while let Some(command) = commands.recv().await {
            let mut done = None;
            let step = match command {
                RoomCommand::Open {
                    conn,
                    token,
                    socket,
                } => {
                    self.sockets.insert(conn, socket);
                    self.engine.on_open(conn, token)
                }
                RoomCommand::Text { conn, text } => self.engine.on_text(conn, &text),
                RoomCommand::Binary { conn } => self.engine.on_binary(conn),
                RoomCommand::Close { conn, done: ack } => {
                    self.sockets.remove(&conn);
                    done = Some(ack);
                    self.engine.on_close(conn)
                }
            };
            let mut host = Host {
                code: &self.code,
                store: &self.store,
                sockets: &mut self.sockets,
            };
            let result = self.engine.run(&mut host, step).await;
            if let Some(done) = done {
                let _ = done.send(());
            }
            if let Err(error) = result {
                // The engine told every socket; the next visitor reloads
                // the room from storage in a new actor.
                log::error!("room {} stopped after storage error: {error}", self.code);
                break;
            }
        }
    }
}

/// The actor's I/O for the engine: SQLite and the socket tasks' queues.
struct Host<'a> {
    code: &'a str,
    store: &'a RoomStore,
    sockets: &'a mut HashMap<ConnId, Socket>,
}

impl RoomHost for Host<'_> {
    type Error = StoreError;

    fn save(&mut self, room: &RoomData) -> impl Future<Output = StoreResult<()>> {
        self.store.save(self.code, room)
    }

    fn deliver(&mut self, effects: Vec<Effect>) -> Vec<ConnId> {
        let mut unreachable = Vec::new();
        for effect in effects {
            match effect {
                Effect::Send { to, message } => {
                    self.queue(to, wire_message(&message), &mut unreachable);
                }
                Effect::Broadcast { to, message } => {
                    let wire = wire_message(&message);
                    for conn in to {
                        self.queue(conn, wire.clone(), &mut unreachable);
                    }
                }
                Effect::Close { conn, reason } => {
                    // Queued, the close frame follows what was sent before
                    // it; a full queue closes right away instead.
                    if let Some(socket) = self.sockets.remove(&conn) {
                        if socket.outgoing.try_send(close_frame(reason)).is_err() {
                            let _ = socket.close.send(Some(reason.into()));
                        }
                    }
                }
            }
        }
        unreachable
    }
}

impl Host<'_> {
    /// Queue `message` for `conn`, or report it: its queue is full, or its
    /// task is gone.
    fn queue(&self, conn: ConnId, message: Message, unreachable: &mut Vec<ConnId>) {
        let queued = self
            .sockets
            .get(&conn)
            .is_some_and(|socket| socket.outgoing.try_send(message).is_ok());
        if !queued {
            unreachable.push(conn);
        }
    }
}

fn wire_message(message: &tantrix_proto::HostMessage) -> Message {
    Message::Text(
        serde_json::to_string(message)
            .expect("host message serializes")
            .into(),
    )
}

fn close_frame(reason: &str) -> Message {
    Message::Close(Some(CloseFrame {
        code: close_code::NORMAL,
        reason: reason.into(),
    }))
}

/// Answer a socket for a code with no room, then close it.
pub async fn missing_room(mut socket: WebSocket) {
    let message = engine::rejected_welcome(RejectCode::RoomNotFound);
    let _ = socket.send(wire_message(&message)).await;
    let _ = socket.send(close_frame("")).await;
}

pub async fn socket_session(socket: WebSocket, handle: Arc<RoomHandle>, token: SessionToken) {
    let conn = ConnId(NEXT_CONN.fetch_add(1, Ordering::Relaxed));
    let (mut writer, mut reader) = socket.split();
    let (outgoing, mut incoming) = mpsc::channel(OUTGOING_CAPACITY);
    let (close, mut closed) = watch::channel::<Option<String>>(None);
    let open = RoomCommand::Open {
        conn,
        token,
        socket: Socket {
            outgoing: outgoing.clone(),
            close,
        },
    };

    if handle.send(open).await {
        loop {
            // Outgoing frames first, so a close reason or a rejection
            // queued before the room stopped still reaches the browser.
            tokio::select! {
                biased;
                message = incoming.recv() => {
                    let Some(message) = message else { break; };
                    let is_close = matches!(message, Message::Close(_));
                    if writer.send(message).await.is_err() || is_close { break; }
                }
                changed = closed.changed() => {
                    if changed.is_ok() {
                        let reason = closed.borrow().clone().unwrap_or_default();
                        let _ = writer.send(close_frame(&reason)).await;
                    }
                    break;
                }
                frame = reader.next() => {
                    let Some(Ok(frame)) = frame else { break; };
                    let command = match frame {
                        // Keepalives never reach the room.
                        Message::Text(text) if text == KEEPALIVE_PING => {
                            let _ = outgoing.try_send(Message::Text(KEEPALIVE_PONG.into()));
                            continue;
                        }
                        Message::Text(text) => RoomCommand::Text { conn, text: text.to_string() },
                        Message::Binary(_) => RoomCommand::Binary { conn },
                        Message::Close(_) => break,
                        Message::Ping(_) | Message::Pong(_) => continue,
                    };
                    if !handle.send(command).await { break; }
                }
                _ = handle.tx.closed() => break,
            }
        }
    }
    let (done, acknowledged) = oneshot::channel();
    if handle.send(RoomCommand::Close { conn, done }).await {
        let _ = acknowledged.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tantrix_game::Color;
    use tantrix_proto::{HostMessage, PeerMessage};

    fn text(message: &PeerMessage) -> String {
        serde_json::to_string(message).unwrap()
    }

    fn test_socket() -> (Socket, mpsc::Receiver<Message>) {
        let (outgoing, messages) = mpsc::channel(8);
        let (close, _closed) = watch::channel(None);
        (Socket { outgoing, close }, messages)
    }

    #[tokio::test]
    async fn a_room_reloads_its_seat_after_the_actor_stops() {
        let store = RoomStore::open(":memory:").unwrap();
        let manager = RoomManager::new(store.clone());
        let OpenRoom::Ready(handle) = manager.open("ABCDEF", true).await.unwrap() else {
            panic!("room should be created");
        };
        let token = SessionToken::new("returning-player-123456");
        let conn = ConnId(1);
        let (socket, mut messages) = test_socket();
        assert!(
            handle
                .send(RoomCommand::Open {
                    conn,
                    token: token.clone(),
                    socket,
                })
                .await
        );
        let hello = text(&PeerMessage::Hello {
            nickname: "Player".into(),
        });
        assert!(handle.send(RoomCommand::Text { conn, text: hello }).await);
        assert!(matches!(messages.recv().await, Some(Message::Text(_))));
        let pick = text(&PeerMessage::PickColor {
            color: Some(Color::Red),
        });
        assert!(handle.send(RoomCommand::Text { conn, text: pick }).await);
        assert!(matches!(messages.recv().await, Some(Message::Text(_))));
        let (done, acknowledged) = oneshot::channel();
        assert!(handle.send(RoomCommand::Close { conn, done }).await);
        acknowledged.await.unwrap();
        drop(handle);
        drop(manager);

        let restarted = RoomManager::new(store);
        let OpenRoom::Ready(handle) = restarted.open("ABCDEF", false).await.unwrap() else {
            panic!("room should be restored");
        };
        let conn = ConnId(2);
        let (socket, mut messages) = test_socket();
        assert!(
            handle
                .send(RoomCommand::Open {
                    conn,
                    token,
                    socket
                })
                .await
        );
        let hello = text(&PeerMessage::Hello {
            nickname: "Ignored on reconnect".into(),
        });
        assert!(handle.send(RoomCommand::Text { conn, text: hello }).await);
        let Some(Message::Text(message)) = messages.recv().await else {
            panic!("expected welcome");
        };
        let HostMessage::Welcome { lobby, .. } = serde_json::from_str(&message).unwrap() else {
            panic!("expected welcome");
        };
        assert_eq!(lobby.seats[Color::Red].as_ref().unwrap().nickname, "Player");
    }
}
