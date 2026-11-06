//! Native room WebSockets. One actor task per room feeds its sockets'
//! events to a `RoomEngine` (one at a time) and carries out its effects;
//! the room rules and orchestration live in `tantrix_room::engine`.
//!
//! Logs: a room's work runs in a `room` span (`room` = its permanent ID) and
//! each socket in a `socket` span (`room`, `conn`, and `peer`, the player's
//! public ID, once joined). Session tokens are never logged.

use std::borrow::Cow;
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use axum::extract::ws::{close_code, CloseFrame, Message, WebSocket};
use futures::{Sink, SinkExt, StreamExt};
use tantrix_proto::{
    HostMessage, RejectCode, RoomApiError, RoomReference, KEEPALIVE_PING, KEEPALIVE_PONG,
};
use tantrix_room::engine::{self, ConnId, Effect, RoomEngine, RoomHost};
use tantrix_room::lifecycle::{self, RoomRecord};
use tantrix_room::room_logic::RoomData;
use tantrix_room::session_cookie::SessionToken;
use tokio::sync::{mpsc, oneshot, watch, Mutex};
use tokio::time::Instant;
use tracing::{Instrument, Span};

use super::store::{RoomStore, StoreError, StoreResult};

const OUTGOING_CAPACITY: usize = 64;
const COMMAND_CAPACITY: usize = 64;

/// A socket must send something other than a keepalive this soon after it
/// opens (clients say `Hello` right away)…
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// …and then some frame at least this often: clients ping every 30 s.
const IDLE_TIMEOUT: Duration = Duration::from_secs(75);
/// A peer that takes longer to accept a frame has stopped reading.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// Close reason for a socket that went quiet.
const TIMED_OUT: &str = "Connection timed out.";

/// Why a socket closed, when the server sent its peer no reason: logged only.
const CLIENT_CLOSED: &str = "closed by the client";
const CONNECTION_LOST: &str = "connection lost";
const STOPPED_READING: &str = "client stopped reading";
const ROOM_STOPPED: &str = "room stopped";
/// The engine closes a socket whose `Hello` it refused without a reason.
const HELLO_REFUSED: &str = "hello refused";

/// Why a room actor stopped.
const IDLE: &str = "no sockets or requests left";
const DELETED: &str = "deleted";
const EXPIRED: &str = "expired";
const STORAGE_FAILED: &str = "storage unavailable";

/// Socket numbers, unique across the process and so within each room.
static NEXT_CONN: AtomicU64 = AtomicU64::new(1);

/// The span of a room's work: its actor, and each request that opens it.
/// Error level, so failures keep their room when `RUST_LOG` hides info.
pub fn room_span(id: &str) -> Span {
    tracing::error_span!(parent: None, "room", room = %id)
}

/// The span of one socket. Its actor records `peer` once the room accepts
/// the socket's `Hello`.
pub(super) fn socket_span(room: &str, conn: ConnId) -> Span {
    tracing::error_span!(
        parent: None,
        "socket",
        room = %room,
        conn = conn.0,
        peer = tracing::field::Empty,
    )
}

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
        async {
            if super::store::unavailable(self.store.insert(id, now).await)? {
                tracing::info!("room created");
            }
            self.inspect(id, now, true).await
        }
        .instrument(room_span(id))
        .await
    }

    pub async fn reference(
        &self,
        id: &str,
        now: u64,
        allocate: bool,
    ) -> Result<RoomReference, RoomApiError> {
        self.inspect(id, now, allocate)
            .instrument(room_span(id))
            .await
    }

    /// [`Self::reference`], in the caller's room span.
    async fn inspect(
        &self,
        id: &str,
        now: u64,
        allocate: bool,
    ) -> Result<RoomReference, RoomApiError> {
        match super::store::unavailable(self.open(id, now).await)? {
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
        let due = self.store.due(now).await?;
        if !due.is_empty() {
            tracing::debug!(rooms = due.len(), "cleanup sweep");
        }
        for id in due {
            match self.reference(&id, now, false).await {
                Ok(_) | Err(RoomApiError::Expired | RoomApiError::NotFound) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub async fn open(&self, id: &str, now: u64) -> StoreResult<OpenRoom> {
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
        // Tombstones and rooms due for deletion need no actor.
        let deletable = record.is_deletable(now);
        let Some(room) = record.room.clone().filter(|_| !deletable) else {
            return match clean(&mut record, &mut self.store.clone(), now).await {
                Ok(()) => Ok(OpenRoom::Expired),
                Err(RoomApiError::NotFound) => Ok(OpenRoom::Missing),
                Err(error) => Err(error.into()),
            };
        };
        let engine = RoomEngine::new(room, []);
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let handle = Arc::new(RoomHandle {
            id: id.to_owned(),
            tx,
        });
        rooms.insert(id.to_owned(), Arc::downgrade(&handle));
        let actor = RoomActor::new(record, engine, self.store.clone());
        tokio::spawn(actor.run(rx).instrument(room_span(id)));
        Ok(OpenRoom::Ready(handle))
    }
}

/// Run the lifecycle cleanup on `record`, logging any expiry or deletion.
async fn clean(
    record: &mut RoomRecord,
    store: &mut RoomStore,
    now: u64,
) -> Result<(), RoomApiError> {
    let had_room = record.room.is_some();
    let had_code = record.invitation.is_some();
    let result = lifecycle::cleanup(record, store, now).await;
    match result {
        // Cleanup reports a deleted record as not found.
        Err(RoomApiError::NotFound) => {
            let record = if had_room {
                "started room"
            } else {
                "tombstone"
            };
            tracing::info!(record, "room deleted");
        }
        Ok(()) if had_room && record.room.is_none() => tracing::info!("lobby expired"),
        Ok(()) if had_code && record.invitation.is_none() => {
            tracing::info!("invitation code released");
        }
        _ => {}
    }
    result
}

pub struct RoomHandle {
    /// The room's permanent ID.
    id: String,
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
    /// The socket task's span, for what the room does with this socket.
    span: Span,
    /// The room accepted its `Hello`, and `span` names its player.
    joined: bool,
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

    /// Serve the room until it stops, in its room span.
    async fn run(mut self, commands: mpsc::Receiver<RoomCommand>) {
        let reason = self.serve(commands).await;
        tracing::info!(reason, "room stopped");
    }

    /// Serve the room; returns why it stopped.
    async fn serve(&mut self, mut commands: mpsc::Receiver<RoomCommand>) -> &'static str {
        // No socket outlives its actor: a room saved as connected starts an
        // inactivity period (startup does this for every room in the store).
        self.record.observe_connections(false, now_ms());
        let loaded = self.engine.on_load();
        let mut host = Host {
            record: &mut self.record,
            store: &self.store,
            sockets: &mut self.sockets,
        };
        let loaded = match self.engine.run(&mut host, loaded).await {
            Ok(()) => self.store.save_record(&self.record).await,
            Err(error) => Err(error),
        };
        if let Err(error) = loaded {
            tracing::error!(%error, "room storage error");
            return STORAGE_FAILED;
        }
        let room = self.engine.room();
        tracing::info!(
            match_id = room.match_id,
            players = room.players.len(),
            started = room.has_started(),
            "room loaded"
        );
        loop {
            let Some(command) = commands.recv().await else {
                return IDLE;
            };
            if let RoomCommand::Inspect {
                now,
                allocate,
                reply,
            } = command
            {
                self.record
                    .observe_connections(self.engine.connections().any(|(_, c)| c.joined), now);
                let result = async {
                    clean(&mut self.record, &mut self.store, now).await?;
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
                // Stop once storage failed, or holds no live room (NotFound: deleted).
                let stop = match &result {
                    Err(RoomApiError::StorageUnavailable) => Some(STORAGE_FAILED),
                    Err(RoomApiError::NotFound) => Some(DELETED),
                    _ if self.record.room.is_none() => Some(EXPIRED),
                    _ => None,
                };
                let _ = reply.send(result);
                match stop {
                    Some(reason) => return reason,
                    None => continue,
                }
            }
            // A text may be the `Hello` that joins its socket.
            let sender = match &command {
                RoomCommand::Text { conn, .. } => Some(*conn),
                _ => None,
            };
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
                RoomCommand::Text { conn, text } => self.engine.on_text(conn, &text, now_ms()),
                RoomCommand::Binary { conn } => self.engine.on_binary(conn, now_ms()),
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
            if let Some(conn) = sender {
                self.note_joined(conn);
            }
            let presence = self
                .record
                .observe_connections(self.engine.connections().any(|(_, c)| c.joined), now_ms());
            // A changed room was saved with the presence it had; save again
            // only for a new presence, and never after a failed run.
            let saved = match &result {
                Ok(()) if presence => self.store.save_record(&self.record).await,
                _ => Ok(()),
            };
            if let Some(done) = done {
                let _ = done.send(());
            }
            if let Err(error) = result.and(saved) {
                tracing::error!(%error, "room storage error");
                let conns: Vec<_> = self.sockets.keys().copied().collect();
                self.host().deliver(engine::storage_unavailable(conns));
                return STORAGE_FAILED;
            }
        }
    }

    /// Name the player in `conn`'s logs once the room has accepted its `Hello`.
    fn note_joined(&mut self, conn: ConnId) {
        let (Some(socket), Some(connection)) =
            (self.sockets.get_mut(&conn), self.engine.connection(conn))
        else {
            return;
        };
        if socket.joined || !connection.joined {
            return;
        }
        socket.joined = true;
        if let Some(player) = self.engine.room().player(&connection.token) {
            socket
                .span
                .record("peer", tracing::field::display(player.id.as_str()));
        }
        socket.span.in_scope(|| tracing::info!("socket joined"));
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

    /// The record takes the new room only once storage holds it, so a
    /// failed save leaves nothing uncommitted for a later save to write.
    fn save(&mut self, room: &RoomData) -> impl Future<Output = StoreResult<()>> {
        let mut next = self.record.clone();
        let archive = next.replace_room(room);
        async move {
            self.store.save_with_archive(&next, archive).await?;
            *self.record = next;
            Ok(())
        }
    }

    fn deliver(&mut self, effects: Vec<Effect>) -> Vec<ConnId> {
        let mut unreachable = Vec::new();
        for effect in effects {
            match effect {
                Effect::Send { to, message } => {
                    if let HostMessage::Welcome {
                        rejected: Some(code),
                        ..
                    } = &message
                    {
                        if let Some(socket) = self.sockets.get(&to) {
                            socket
                                .span
                                .in_scope(|| tracing::info!(?code, "hello refused"));
                        }
                    }
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

/// Close a socket the handshake refused, with a reason the player reads.
pub async fn refused(mut socket: WebSocket, reason: &'static str) {
    write(&mut socket, close_frame(reason)).await;
}

/// Answer a socket for a missing or expired room, then close it.
pub async fn rejected_room(mut socket: WebSocket, code: RejectCode) {
    let message = engine::rejected_welcome(code);
    if write(&mut socket, wire_message(&message)).await {
        write(&mut socket, close_frame("")).await;
    }
}

/// Write one frame, giving up on a peer that stopped reading.
async fn write<S: Sink<Message> + Unpin>(writer: &mut S, message: Message) -> bool {
    matches!(
        tokio::time::timeout(SEND_TIMEOUT, writer.send(message)).await,
        Ok(Ok(()))
    )
}

/// When a socket must have sent its next frame, so that sockets that never
/// say anything, or whose peer vanished without closing, don't stay open
/// (and their player shown as connected).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Liveness {
    greeted: bool,
    deadline: Instant,
}

impl Liveness {
    fn new(now: Instant) -> Self {
        Self {
            greeted: false,
            deadline: now + HELLO_TIMEOUT,
        }
    }

    /// A frame arrived at `now`. Keepalives keep a greeted socket open but
    /// are not its greeting.
    fn on_frame(&mut self, now: Instant, keepalive: bool) {
        self.greeted |= !keepalive;
        if self.greeted {
            self.deadline = now + IDLE_TIMEOUT;
        }
    }
}

pub async fn socket_session(socket: WebSocket, handle: Arc<RoomHandle>, token: SessionToken) {
    let conn = ConnId(NEXT_CONN.fetch_add(1, Ordering::Relaxed));
    let span = socket_span(&handle.id, conn);
    serve_socket(socket, handle, token, conn, span.clone())
        .instrument(span)
        .await;
}

async fn serve_socket(
    socket: WebSocket,
    handle: Arc<RoomHandle>,
    token: SessionToken,
    conn: ConnId,
    span: Span,
) {
    tracing::info!("socket opened");
    let (mut writer, mut reader) = socket.split();
    let (outgoing, mut incoming) = mpsc::channel(OUTGOING_CAPACITY);
    let (close, mut closed) = watch::channel::<Option<String>>(None);
    let open = RoomCommand::Open {
        conn,
        token,
        socket: Socket {
            outgoing: outgoing.clone(),
            close,
            span,
            joined: false,
        },
    };

    let reason: Cow<'static, str> = if handle.send(open).await {
        let mut liveness = Liveness::new(Instant::now());
        loop {
            // Outgoing frames first, so a close reason or a rejection
            // queued before the room stopped still reaches the browser.
            tokio::select! {
                biased;
                message = incoming.recv() => {
                    let Some(message) = message else { break ROOM_STOPPED.into(); };
                    let closing = close_reason(&message);
                    let written = write(&mut writer, message).await;
                    if let Some(reason) = closing { break reason.into(); }
                    if !written { break STOPPED_READING.into(); }
                }
                changed = closed.changed() => {
                    if changed.is_err() { break ROOM_STOPPED.into(); }
                    let reason = closed.borrow().clone().unwrap_or_default();
                    write(&mut writer, close_frame(&reason)).await;
                    break reason.into();
                }
                frame = reader.next() => {
                    let Some(Ok(frame)) = frame else { break CONNECTION_LOST.into(); };
                    let keepalive = match &frame {
                        Message::Text(text) => text.as_str() == KEEPALIVE_PING,
                        Message::Ping(_) | Message::Pong(_) => true,
                        Message::Binary(_) | Message::Close(_) => false,
                    };
                    liveness.on_frame(Instant::now(), keepalive);
                    let command = match frame {
                        // Keepalives never reach the room.
                        Message::Text(text) if text == KEEPALIVE_PING => {
                            let _ = outgoing.try_send(Message::Text(KEEPALIVE_PONG.into()));
                            continue;
                        }
                        Message::Text(text) => {
                            tracing::debug!(bytes = text.len(), "text frame");
                            RoomCommand::Text { conn, text: text.to_string() }
                        }
                        Message::Binary(data) => {
                            tracing::debug!(bytes = data.len(), "binary frame");
                            RoomCommand::Binary { conn }
                        }
                        Message::Close(_) => break CLIENT_CLOSED.into(),
                        Message::Ping(_) | Message::Pong(_) => continue,
                    };
                    if !handle.send(command).await { break ROOM_STOPPED.into(); }
                }
                _ = tokio::time::sleep_until(liveness.deadline) => {
                    write(&mut writer, close_frame(TIMED_OUT)).await;
                    break TIMED_OUT.into();
                }
                _ = handle.tx.closed() => break ROOM_STOPPED.into(),
            }
        }
    } else {
        ROOM_STOPPED.into()
    };
    log_closed(&reason);
    let (done, acknowledged) = oneshot::channel();
    if handle.send(RoomCommand::Close { conn, done }).await {
        let _ = acknowledged.await;
    }
}

/// The reason in a close frame the room queued for its peer.
fn close_reason(message: &Message) -> Option<String> {
    let Message::Close(frame) = message else {
        return None;
    };
    Some(
        frame
            .as_ref()
            .map_or_else(String::new, |frame| frame.reason.as_str().to_owned()),
    )
}

/// Log why a socket closed: the reason the server sent its peer, or what
/// its task saw. Limits hit and failures are warnings.
pub(super) fn log_closed(reason: &str) {
    let reason = if reason.is_empty() {
        HELLO_REFUSED
    } else {
        reason
    };
    let failure = [
        engine::TOO_MANY_MESSAGES,
        engine::TOO_MANY_CONNECTIONS,
        engine::TOO_SLOW,
        engine::STORAGE_UNAVAILABLE,
        STOPPED_READING,
    ]
    .contains(&reason);
    if failure {
        tracing::warn!(reason, "socket closed");
    } else {
        tracing::info!(reason, "socket closed");
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

    #[test]
    fn silent_and_vanished_sockets_time_out() {
        let start = Instant::now();
        let at = |seconds| start + Duration::from_secs(seconds);
        let mut liveness = Liveness::new(start);
        assert_eq!(liveness.deadline, start + HELLO_TIMEOUT);
        // Keepalives before a greeting don't keep a socket open.
        liveness.on_frame(at(5), true);
        assert_eq!(liveness.deadline, start + HELLO_TIMEOUT);
        // Once greeted, any frame does, keepalives included.
        liveness.on_frame(at(6), false);
        assert_eq!(liveness.deadline, at(6) + IDLE_TIMEOUT);
        liveness.on_frame(at(36), true);
        assert_eq!(liveness.deadline, at(36) + IDLE_TIMEOUT);
        // Clients ping every 30 s; one missed ping must not close them.
        assert!(IDLE_TIMEOUT > Duration::from_secs(60));
    }

    #[tokio::test]
    async fn a_failed_save_leaves_the_record_as_storage_has_it() {
        let store = RoomStore::open(":memory:").unwrap();
        let mut sockets = HashMap::new();
        let next = RoomData {
            match_id: 7,
            ..RoomData::default()
        };
        // Storage has no row for this room yet, so saving it fails.
        let mut record = RoomRecord::new("game".into(), 0);
        let mut host = Host {
            record: &mut record,
            store: &store,
            sockets: &mut sockets,
        };
        assert!(host.save(&next).await.is_err());
        assert_eq!(record.room.as_ref().map(|room| room.match_id), Some(0));

        store.insert("game", 0).await.unwrap();
        let mut host = Host {
            record: &mut record,
            store: &store,
            sockets: &mut sockets,
        };
        host.save(&next).await.unwrap();
        assert_eq!(record.room.as_ref().map(|room| room.match_id), Some(7));
        let stored = store.load("game").await.unwrap().unwrap();
        assert_eq!(stored.room.map(|room| room.match_id), Some(7));
    }

    fn test_socket() -> (Socket, mpsc::Receiver<Message>) {
        let (outgoing, messages) = mpsc::channel(8);
        let (close, _closed) = watch::channel(None);
        let socket = Socket {
            outgoing,
            close,
            span: Span::none(),
            joined: false,
        };
        (socket, messages)
    }

    #[tokio::test]
    async fn rematches_archive_history_and_keep_the_room_after_reload() {
        use std::collections::HashSet;
        use tantrix_game::{Game, Player};
        use tantrix_proto::EndReason;
        use tantrix_room::room_logic::RoomData;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("rooms.sqlite3");
        let store = RoomStore::open(&path).unwrap();
        let manager = RoomManager::new(store.clone());
        let id = "01234567-89ab-4def-8123-456789abcdef";
        let reference = manager.create(id, now_ms()).await.unwrap();
        let red = SessionToken::new("red-token-123456789");
        let blue = SessionToken::new("blue-token-123456789");
        let mut room = RoomData::default();
        for (token, color) in [(&red, Color::Red), (&blue, Color::Blue)] {
            room.hello(
                token.clone(),
                color.to_string(),
                tantrix_proto::PeerId::new(color.to_string()),
            )
            .unwrap();
            room.seats[color] = Some(token.clone());
        }
        let mut game = Game::new_seeded(
            Player::Someone,
            Player::Someone,
            Player::None,
            Player::None,
            42,
        )
        .unwrap();
        game.play_move(game.legal_moves()[0].placement()).unwrap();
        room.game = Some(game.to_data());
        room.ended = true;
        room.end_reason = Some(EndReason::EndedEarly {
            by: Color::Red,
            winners: game.winners(),
        });
        let finished = serde_json::to_value(room.completed_match().unwrap()).unwrap();
        let mut record = store.load(id).await.unwrap().unwrap();
        record.room = Some(room);
        store.save_record(&record).await.unwrap();

        let OpenRoom::Ready(handle) = manager.open(id, 0).await.unwrap() else {
            panic!("room exists")
        };
        let mut receivers = Vec::new();
        for (conn, token) in [(ConnId(1), red), (ConnId(2), blue)] {
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
            assert!(
                handle
                    .send(RoomCommand::Text {
                        conn,
                        text: text(&PeerMessage::Hello {
                            nickname: "Player".into()
                        })
                    })
                    .await
            );
            loop {
                let Some(Message::Text(message)) = messages.recv().await else {
                    panic!("expected results")
                };
                if matches!(
                    serde_json::from_str::<HostMessage>(&message).unwrap(),
                    HostMessage::GameEnded { .. }
                ) {
                    break;
                }
            }
            receivers.push(messages);
        }
        // Each transition goes through the actor and its real persistence host.
        for match_id in 0..2 {
            if match_id == 1 {
                assert!(
                    handle
                        .send(RoomCommand::Text {
                            conn: ConnId(2),
                            text: text(&PeerMessage::EndGame { match_id })
                        })
                        .await
                );
            }
            assert!(
                handle
                    .send(RoomCommand::Text {
                        conn: ConnId(1),
                        text: text(&PeerMessage::Rematch { match_id })
                    })
                    .await
            );
            for messages in &mut receivers {
                loop {
                    let Some(Message::Text(message)) = messages.recv().await else {
                        panic!("expected lobby")
                    };
                    if let HostMessage::LobbyUpdate { lobby } =
                        serde_json::from_str(&message).unwrap()
                    {
                        assert_eq!(lobby.match_id, match_id + 1);
                        assert!(lobby.seats[Color::Red].is_some());
                        assert!(lobby.seats[Color::Blue].is_some());
                        break;
                    }
                }
            }
            assert!(
                handle
                    .send(RoomCommand::Text {
                        conn: ConnId(1),
                        text: text(&PeerMessage::StartGame {
                            match_id: match_id + 1
                        })
                    })
                    .await
            );
            for messages in &mut receivers {
                loop {
                    let Some(Message::Text(message)) = messages.recv().await else {
                        panic!("expected game")
                    };
                    if let HostMessage::GameStarted { snapshot } =
                        serde_json::from_str(&message).unwrap()
                    {
                        assert_eq!(snapshot.match_id, match_id + 1);
                        assert!(snapshot.board.is_empty());
                        break;
                    }
                }
            }
        }
        let reloaded = RoomStore::open(&path).unwrap();
        let archived = reloaded.load_match(id, 0).await.unwrap().unwrap();
        assert_eq!(serde_json::to_value(&archived).unwrap(), finished);
        assert_eq!(Game::from_data(archived.game).history().len(), 1);
        assert!(reloaded.load_match(id, 1).await.unwrap().is_some());
        let saved = reloaded.load(id).await.unwrap().unwrap();
        assert_eq!(saved.reference().unwrap(), reference);
        let active = saved.room.unwrap();
        assert_eq!(active.match_id, 2);
        assert!(active.snapshot(&HashSet::new()).unwrap().board.is_empty());
        for conn in [ConnId(1), ConnId(2)] {
            let (done, acknowledged) = oneshot::channel();
            handle.send(RoomCommand::Close { conn, done }).await;
            acknowledged.await.unwrap();
        }
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
            .open("01234567-89ab-4def-8123-456789abcdef", 0)
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
            .open("01234567-89ab-4def-8123-456789abcdef", 0)
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

    #[tokio::test]
    async fn the_sweep_deletes_old_tombstones_and_long_inactive_rooms() {
        use super::super::store::tests::finished_room;
        use tantrix_room::lifecycle::{IDLE_TTL_MS, RETENTION_MS, TOMBSTONE_TTL_MS};
        let store = RoomStore::open(":memory:").unwrap();
        let manager = RoomManager::new(store.clone());
        let lobby = "01234567-89ab-4def-8123-456789abcdef";
        let played = "11234567-89ab-4def-8123-456789abcdef";
        manager.create(lobby, 0).await.unwrap();
        finished_room(&store, played, 0).await;
        manager.cleanup(IDLE_TTL_MS).await.unwrap();
        manager
            .cleanup(IDLE_TTL_MS + TOMBSTONE_TTL_MS)
            .await
            .unwrap();
        assert!(store.load(lobby).await.unwrap().is_none());
        assert!(store.load(played).await.unwrap().is_some());
        manager.cleanup(RETENTION_MS).await.unwrap();
        assert_eq!(
            manager.reference(played, RETENTION_MS, false).await,
            Err(RoomApiError::NotFound)
        );
    }
}
