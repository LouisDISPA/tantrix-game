//! Native room WebSockets. One actor task per room feeds its sockets'
//! events to a `RoomEngine` (one at a time) and carries out its effects;
//! the room rules and orchestration live in `tantrix_room::engine`.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use axum::extract::ws::{close_code, CloseFrame, Message, WebSocket};
use futures::{SinkExt, StreamExt};
use tantrix_proto::{
    HostMessage, RejectCode, RoomApiError, RoomReference, KEEPALIVE_PING, KEEPALIVE_PONG,
};
use tantrix_room::engine::{self, ConnId, Effect, RoomEngine, RoomHost};
use tantrix_room::lifecycle::{self, RoomRecord};
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
    Expired,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl RoomManager {
    pub fn new(store: RoomStore) -> Self {
        Self {
            rooms: Mutex::new(HashMap::new()),
            store,
        }
    }

    pub async fn create(&self, id: &str, now: u64) -> Result<RoomReference, RoomApiError> {
        super::store::unavailable(self.store.insert(id, now).await)?;
        self.reference(id, now, true).await
    }

    pub async fn reference(
        &self,
        id: &str,
        now: u64,
        allocate: bool,
    ) -> Result<RoomReference, RoomApiError> {
        match super::store::unavailable(self.open(id).await)? {
            OpenRoom::Ready(handle) => handle.inspect(now, allocate).await,
            OpenRoom::Missing => Err(RoomApiError::NotFound),
            OpenRoom::Expired => Err(RoomApiError::Expired),
        }
    }

    pub async fn resolve(&self, code: &str, now: u64) -> Result<RoomReference, RoomApiError> {
        let id = super::store::unavailable(self.store.find_code(code).await)?
            .ok_or(RoomApiError::NotFound)?;
        let reference = self.reference(&id, now, false).await?;
        if reference.code.as_deref() != Some(code) {
            return Err(RoomApiError::NotFound);
        }
        Ok(reference)
    }

    pub async fn cleanup(&self, now: u64) -> StoreResult<()> {
        for id in self.store.due(now).await? {
            match self.reference(&id, now, false).await {
                Ok(_) | Err(RoomApiError::Expired | RoomApiError::NotFound) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub async fn open(&self, id: &str) -> StoreResult<OpenRoom> {
        let mut rooms = self.rooms.lock().await;
        rooms.retain(|_, handle| handle.strong_count() > 0);
        if let Some(handle) = rooms.get(id).and_then(Weak::upgrade) {
            if !handle.tx.is_closed() {
                return Ok(OpenRoom::Ready(handle));
            }
        }
        let Some(mut record) = self.store.load(id).await? else {
            return Ok(OpenRoom::Missing);
        };
        let Some(room) = record.room.clone() else {
            lifecycle::cleanup(&mut record, &mut self.store.clone(), now_ms()).await?;
            return Ok(OpenRoom::Expired);
        };
        let engine = RoomEngine::new(room, []);
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let handle = Arc::new(RoomHandle { tx });
        rooms.insert(id.to_owned(), Arc::downgrade(&handle));
        tokio::spawn(RoomActor::new(record, engine, self.store.clone()).run(rx));
        Ok(OpenRoom::Ready(handle))
    }
}

pub struct RoomHandle {
    tx: mpsc::Sender<RoomCommand>,
}

impl RoomHandle {
    pub async fn inspect(&self, now: u64, allocate: bool) -> Result<RoomReference, RoomApiError> {
        let (reply, answer) = oneshot::channel();
        if !self
            .send(RoomCommand::Inspect {
                now,
                allocate,
                reply,
            })
            .await
        {
            return Err(RoomApiError::StorageUnavailable);
        }
        answer
            .await
            .unwrap_or(Err(RoomApiError::StorageUnavailable))
    }
    async fn send(&self, command: RoomCommand) -> bool {
        self.tx.send(command).await.is_ok()
    }
}

/// One socket event, in the order its socket task saw it.
enum RoomCommand {
    Inspect {
        now: u64,
        allocate: bool,
        reply: oneshot::Sender<Result<RoomReference, RoomApiError>>,
    },
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
    record: RoomRecord,
    engine: RoomEngine,
    sockets: HashMap<ConnId, Socket>,
    store: RoomStore,
}

impl RoomActor {
    fn new(record: RoomRecord, engine: RoomEngine, store: RoomStore) -> Self {
        Self {
            record,
            engine,
            sockets: HashMap::new(),
            store,
        }
    }

    fn host(&mut self) -> Host<'_> {
        Host {
            record: &mut self.record,
            store: &self.store,
            sockets: &mut self.sockets,
        }
    }

    async fn run(mut self, mut commands: mpsc::Receiver<RoomCommand>) {
        // A native restart loses all sockets, but does not reset an existing
        // inactivity deadline. A previously connected room gets a fresh deadline.
        self.record.observe_connections(false, now_ms());
        let loaded = self.engine.on_load();
        let mut host = Host {
            record: &mut self.record,
            store: &self.store,
            sockets: &mut self.sockets,
        };
        if self.engine.run(&mut host, loaded).await.is_err() {
            return;
        }
        if self.store.save_record(&self.record).await.is_err() {
            return;
        }
        while let Some(command) = commands.recv().await {
            if let RoomCommand::Inspect {
                now,
                allocate,
                reply,
            } = command
            {
                self.record
                    .observe_connections(self.engine.connections().any(|(_, c)| c.joined), now);
                let result = async {
                    lifecycle::cleanup(&mut self.record, &mut self.store, now).await?;
                    self.record.reference()?;
                    if allocate {
                        if self.record.invitation.is_none() && self.record.vacant_since.is_some() {
                            self.record.vacant_since = Some(now);
                        }
                        match lifecycle::allocate(
                            &mut self.record,
                            &mut self.store,
                            lifecycle::new_invitation,
                        )
                        .await
                        {
                            Err(RoomApiError::AllocationBusy)
                                if self.engine.room().game.is_some() => {}
                            result => result?,
                        }
                    }
                    self.record.reference()
                }
                .await;
                let stop = matches!(result, Err(RoomApiError::StorageUnavailable))
                    || self.record.room.is_none();
                let _ = reply.send(result);
                if stop {
                    break;
                }
                continue;
            }
            let mut done = None;
            let step = match command {
                RoomCommand::Open {
                    conn,
                    token,
                    socket,
                } => {
                    if let Ok(room) = self.record.reference() {
                        let _ = socket
                            .outgoing
                            .try_send(wire_message(&HostMessage::RoomInfo { room }));
                    }
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
                RoomCommand::Inspect { .. } => unreachable!(),
            };
            let mut host = Host {
                record: &mut self.record,
                store: &self.store,
                sockets: &mut self.sockets,
            };
            let result = self.engine.run(&mut host, step).await;
            self.record
                .observe_connections(self.engine.connections().any(|(_, c)| c.joined), now_ms());
            let saved = self.store.save_record(&self.record).await;
            if let Some(done) = done {
                let _ = done.send(());
            }
            if let Err(error) = result.and(saved) {
                log::error!("room stopped after storage error: {error}");
                let conns: Vec<_> = self.sockets.keys().copied().collect();
                self.host().deliver(engine::storage_unavailable(conns));
                break;
            }
        }
    }
}

/// The actor's I/O for the engine: SQLite and the socket tasks' queues.
struct Host<'a> {
    record: &'a mut RoomRecord,
    store: &'a RoomStore,
    sockets: &'a mut HashMap<ConnId, Socket>,
}

impl RoomHost for Host<'_> {
    type Error = StoreError;

    fn save(&mut self, room: &RoomData) -> impl Future<Output = StoreResult<()>> {
        self.record.room = Some(room.clone());
        self.store.save_record(self.record)
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
pub async fn missing_room(socket: WebSocket) {
    rejected_room(socket, RejectCode::RoomNotFound).await;
}

pub async fn rejected_room(mut socket: WebSocket, code: RejectCode) {
    let message = engine::rejected_welcome(code);
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
        manager
            .create("01234567-89ab-4def-8123-456789abcdef", now_ms())
            .await
            .unwrap();
        let OpenRoom::Ready(handle) = manager
            .open("01234567-89ab-4def-8123-456789abcdef")
            .await
            .unwrap()
        else {
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
        let OpenRoom::Ready(handle) = restarted
            .open("01234567-89ab-4def-8123-456789abcdef")
            .await
            .unwrap()
        else {
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
        let _info = messages.recv().await;
        let Some(Message::Text(message)) = messages.recv().await else {
            panic!("expected welcome");
        };
        let HostMessage::Welcome { lobby, .. } = serde_json::from_str(&message).unwrap() else {
            panic!("expected welcome");
        };
        assert_eq!(lobby.seats[Color::Red].as_ref().unwrap().nickname, "Player");
    }
    #[tokio::test]
    async fn cleanup_preserves_saved_games_and_resume_renews_the_invitation_deadline() {
        use tantrix_game::{Game, Player};
        use tantrix_room::lifecycle::IDLE_TTL_MS;
        let store = RoomStore::open(":memory:").unwrap();
        let manager = RoomManager::new(store.clone());
        let id = "01234567-89ab-4def-8123-456789abcdef";
        let original = manager.create(id, 0).await.unwrap();
        let mut record = store.load(id).await.unwrap().unwrap();
        let game = Game::new_seeded(
            Player::Someone,
            Player::Someone,
            Player::None,
            Player::None,
            42,
        )
        .unwrap();
        record.room.as_mut().unwrap().game = Some(game.to_data());
        let before = serde_json::to_value(record.room.as_ref().unwrap()).unwrap();
        store.save_record(&record).await.unwrap();
        manager.cleanup(IDLE_TTL_MS).await.unwrap();
        assert!(store
            .find_code(original.code.as_deref().unwrap())
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            serde_json::to_value(store.load(id).await.unwrap().unwrap().room.unwrap()).unwrap(),
            before
        );
        let resumed = manager.reference(id, IDLE_TTL_MS + 1, true).await.unwrap();
        assert_eq!(resumed.id, original.id);
        assert!(resumed.code.is_some());
        let record = store.load(id).await.unwrap().unwrap();
        assert_eq!(record.deadline(), Some(2 * IDLE_TTL_MS + 1));
        assert_eq!(serde_json::to_value(record.room.unwrap()).unwrap(), before);
    }

    #[tokio::test]
    async fn abandoned_creation_expires_and_an_old_retry_does_not_revive_it() {
        use tantrix_room::lifecycle::IDLE_TTL_MS;
        let store = RoomStore::open(":memory:").unwrap();
        let manager = RoomManager::new(store);
        let id = "01234567-89ab-4def-8123-456789abcdef";
        let room = manager.create(id, 0).await.unwrap();
        manager.cleanup(IDLE_TTL_MS).await.unwrap();
        assert_eq!(
            manager.create(id, IDLE_TTL_MS + 1).await,
            Err(RoomApiError::Expired)
        );
        assert_eq!(
            manager
                .resolve(room.code.as_deref().unwrap(), IDLE_TTL_MS + 1)
                .await,
            Err(RoomApiError::NotFound)
        );
    }
}
