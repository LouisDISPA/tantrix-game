//! Hibernatable, authoritative room Durable Object. A `RoomEngine`
//! (`tantrix_room::engine`) decides what every socket event does; this
//! object owns the sockets, their attachments, keepalive auto-responses and
//! Durable Object storage.

use std::collections::BTreeMap;
use std::future::Future;

use crate::catalog::{self, WorkerCatalog};
use futures::lock::Mutex;
use serde::{Deserialize, Serialize};
use tantrix_proto::{HostMessage, RejectCode, RoomApiError, RoomReference};
use tantrix_room::engine::{self, ConnId, Connection, Effect, RoomEngine, RoomHost};
use tantrix_room::handshake::{self, Refusal};
use tantrix_room::lifecycle::{self, RoomRecord};
use tantrix_room::room_logic::RoomData;
use tantrix_room::session_cookie;
use worker::{
    console_error, durable_object, DurableObject, Env, Request, Response, Result, State, Storage,
    WebSocket, WebSocketIncomingMessage, WebSocketPair,
};

const STORAGE_KEY: &str = "record";

struct LiveRoom {
    record: RoomRecord,
    engine: Option<RoomEngine>,
}

/// Stored on each socket so it survives hibernation.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Attachment {
    conn: ConnId,
    connection: Connection,
    /// The room is done with this socket (either side closed it). It can
    /// still be listed until its close completes.
    left: bool,
}

fn attachment(ws: &WebSocket) -> Option<Attachment> {
    ws.deserialize_attachment().ok().flatten()
}

/// Mark the socket as done, so a rebuilt engine leaves it out.
fn retire(ws: &WebSocket, mut attachment: Attachment) {
    attachment.left = true;
    let _ = ws.serialize_attachment(&attachment);
}

fn json(message: &HostMessage) -> String {
    serde_json::to_string(message).expect("host message serializes")
}

fn refuse(refusal: Refusal) -> Result<Response> {
    Response::error(refusal.message(), refusal.status())
}

#[durable_object]
pub struct Room {
    state: State,
    /// The room and its sockets while the object is awake. After
    /// hibernation it is rebuilt from storage and the socket attachments.
    engine: Mutex<Option<LiveRoom>>,
    env: Env,
}

/// The object's I/O for the engine: storage, and the sockets the room
/// still serves.
struct Host<'a> {
    storage: Storage,
    record: &'a mut RoomRecord,
    sockets: BTreeMap<ConnId, (WebSocket, Attachment)>,
}

impl<'a> Host<'a> {
    fn new(state: &State, record: &'a mut RoomRecord) -> Self {
        let sockets = state
            .get_websockets()
            .into_iter()
            .filter_map(|ws| {
                let attachment = attachment(&ws).filter(|a| !a.left)?;
                Some((attachment.conn, (ws, attachment)))
            })
            .collect();
        Self {
            storage: state.storage(),
            record,
            sockets,
        }
    }

    fn connections(&self) -> impl Iterator<Item = (ConnId, Connection)> + '_ {
        self.sockets
            .iter()
            .map(|(conn, (_, attachment))| (*conn, attachment.connection.clone()))
    }

    /// Send `json` to `conn`, or report it: the runtime no longer lists
    /// the socket, or refuses the send (it is closing).
    fn send(&self, conn: ConnId, json: &str, unreachable: &mut Vec<ConnId>) {
        let sent = self
            .sockets
            .get(&conn)
            .is_some_and(|(ws, _)| ws.send_with_str(json).is_ok());
        if !sent {
            unreachable.push(conn);
        }
    }
}

impl RoomHost for Host<'_> {
    type Error = worker::Error;

    fn save(&mut self, room: &RoomData) -> impl Future<Output = Result<()>> {
        let archive = self.record.replace_room(room);
        catalog::persist_with_archive(&self.storage, self.record, archive)
    }

    fn deliver(&mut self, effects: Vec<Effect>) -> Vec<ConnId> {
        let mut unreachable = Vec::new();
        for effect in effects {
            match effect {
                Effect::Send { to, message } => self.send(to, &json(&message), &mut unreachable),
                Effect::Broadcast { to, message } => {
                    let json = json(&message);
                    for conn in to {
                        self.send(conn, &json, &mut unreachable);
                    }
                }
                Effect::Close { conn, reason } => {
                    if let Some((ws, attachment)) = self.sockets.remove(&conn) {
                        retire(&ws, attachment);
                        let _ = ws.close(Some(1000), Some(reason));
                    }
                }
            }
        }
        unreachable
    }
}

impl Room {
    fn close_failed_sockets(&self) {
        for ws in self.state.get_websockets() {
            if let Some(a) = attachment(&ws) {
                retire(&ws, a);
            }
            let _ = ws.close(Some(1000), Some(engine::STORAGE_UNAVAILABLE));
        }
    }
    async fn load(&self, cache: &mut Option<LiveRoom>) -> Result<()> {
        if cache.is_some() {
            return Ok(());
        }
        let Some(mut record) = self.state.storage().get::<RoomRecord>(STORAGE_KEY).await? else {
            return Ok(());
        };
        let mut host = Host::new(&self.state, &mut record);
        let mut engine = host
            .record
            .room
            .clone()
            .map(|room| RoomEngine::new(room, host.connections()));
        if let Some(engine) = &mut engine {
            let loaded = engine.on_load();
            engine.run(&mut host, loaded).await?;
        }
        let connected = engine
            .as_ref()
            .is_some_and(|e| e.connections().any(|(_, c)| c.joined));
        record.observe_connections(connected, catalog::now_ms());
        catalog::persist(&self.state.storage(), &record).await?;
        *cache = Some(LiveRoom { record, engine });
        Ok(())
    }

    async fn settle(
        &self,
        live: &mut LiveRoom,
        allocate: bool,
    ) -> std::result::Result<RoomReference, RoomApiError> {
        let connected = live
            .engine
            .as_ref()
            .is_some_and(|e| e.connections().any(|(_, c)| c.joined));
        live.record
            .observe_connections(connected, catalog::now_ms());
        let mut catalog = WorkerCatalog {
            storage: self.state.storage(),
            env: self.env.clone(),
        };
        lifecycle::cleanup(&mut live.record, &mut catalog, catalog::now_ms()).await?;
        if live.record.room.is_none() {
            live.engine = None;
            for ws in self.state.get_websockets() {
                if let Some(a) = attachment(&ws) {
                    retire(&ws, a);
                }
                let _ = ws.close(Some(1000), Some("Lobby expired."));
            }
            return Err(RoomApiError::Expired);
        }
        if allocate {
            if live.record.invitation.is_none() && live.record.vacant_since.is_some() {
                live.record.vacant_since = Some(catalog::now_ms());
            }
            match lifecycle::allocate(&mut live.record, &mut catalog, lifecycle::new_invitation)
                .await
            {
                Err(RoomApiError::AllocationBusy)
                    if live.record.room.as_ref().is_some_and(|r| r.game.is_some()) => {}
                result => result?,
            }
        }
        live.record.reference()
    }

    async fn sync_presence(&self, live: &mut LiveRoom) -> Result<()> {
        let connected = live
            .engine
            .as_ref()
            .is_some_and(|e| e.connections().any(|(_, c)| c.joined));
        live.record
            .observe_connections(connected, catalog::now_ms());
        catalog::persist(&self.state.storage(), &live.record).await
    }

    /// A number no socket of this object has used: attachments of closing
    /// sockets keep theirs.
    fn next_conn(&self) -> ConnId {
        let last = self
            .state
            .get_websockets()
            .iter()
            .filter_map(attachment)
            .map(|attachment| attachment.conn.0)
            .max();
        ConnId(last.map_or(1, |last| last + 1))
    }
}

impl DurableObject for Room {
    fn new(state: State, env: Env) -> Self {
        // Client keepalives are answered by the runtime without waking the
        // object, so idle rooms cost no duration while sockets stay open.
        if let Ok(pair) = worker::worker_sys::WebSocketRequestResponsePair::new(
            tantrix_proto::KEEPALIVE_PING,
            tantrix_proto::KEEPALIVE_PONG,
        ) {
            state.set_websocket_auto_response(&pair);
        }
        Self {
            state,
            engine: Mutex::new(None),
            env,
        }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        let url = req.url()?;
        let mut cache = self.engine.lock().await;
        if let Err(error) = self.load(&mut cache).await {
            console_error!("room load failed: {error}");
            self.close_failed_sockets();
            *cache = None;
            return crate::rooms_api::error(RoomApiError::StorageUnavailable);
        }
        if let Some(id) = url.path().strip_prefix("/create/") {
            if !tantrix_proto::is_game_id(id) {
                return crate::rooms_api::error(RoomApiError::InvalidRequest);
            }
            if cache.is_none() {
                let record = RoomRecord::new(id.into(), catalog::now_ms());
                catalog::persist(&self.state.storage(), &record).await?;
                *cache = Some(LiveRoom {
                    record,
                    engine: Some(RoomEngine::default()),
                });
            }
        }
        let websocket = req.headers().get("Upgrade")?.as_deref() == Some("websocket");
        if url.path().starts_with("/room/") && !websocket {
            return Response::error("expected websocket", 426);
        }
        if websocket {
            let id = url.path().strip_prefix("/room/").unwrap_or_default();
            if let Err(refusal) = handshake::check(id) {
                return refuse(refusal);
            }
        }
        let Some(live) = cache.as_mut() else {
            if !websocket {
                return crate::rooms_api::error(RoomApiError::NotFound);
            }
            return rejected(RejectCode::RoomNotFound);
        };
        let allocate =
            websocket || url.path().starts_with("/create/") || url.query() == Some("allocate=1");
        let reference = match self.settle(live, allocate).await {
            Ok(reference) => reference,
            Err(error) => {
                if error == RoomApiError::StorageUnavailable {
                    self.close_failed_sockets();
                    *cache = None;
                }
                if websocket && error == RoomApiError::Expired {
                    return rejected(RejectCode::RoomExpired);
                }
                return crate::rooms_api::error(error);
            }
        };
        if !websocket {
            return Response::from_json(&reference);
        }
        let cookie = req.headers().get("Cookie")?;
        let (token, set_cookie) =
            session_cookie::identify(cookie.as_deref(), url.scheme() == "https");
        let pair = WebSocketPair::new()?;
        let conn = self.next_conn();
        self.state.accept_web_socket(&pair.server);
        let attachment = Attachment {
            conn,
            connection: Connection {
                token: token.clone(),
                joined: false,
            },
            left: false,
        };
        pair.server.serialize_attachment(&attachment)?;
        pair.server
            .send_with_str(json(&HostMessage::RoomInfo { room: reference }))?;
        let mut host = Host::new(&self.state, &mut live.record);
        host.sockets.insert(conn, (pair.server.clone(), attachment));
        let engine = live.engine.as_mut().expect("live room has an engine");
        let opened = engine.on_open(conn, token);
        engine.run(&mut host, opened).await?;
        let mut response = Response::from_websocket(pair.client)?;
        if let Some(cookie) = set_cookie {
            response.headers_mut().set("Set-Cookie", &cookie)?;
        }
        Ok(response)
    }

    async fn alarm(&self) -> Result<Response> {
        let mut cache = self.engine.lock().await;
        if let Err(error) = self.load(&mut cache).await {
            self.close_failed_sockets();
            *cache = None;
            return Err(error);
        }
        if let Some(live) = cache.as_mut() {
            let result = self.settle(live, false).await;
            match result {
                Ok(_) | Err(RoomApiError::Expired) => {}
                Err(error) => {
                    self.close_failed_sockets();
                    *cache = None;
                    return Err(worker::Error::RustError(error.to_string()));
                }
            }
        }
        Response::ok("ok")
    }

    async fn websocket_message(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        let Some(attachment) = attachment(&ws).filter(|a| !a.left) else {
            return Ok(());
        };
        let conn = attachment.conn;
        let mut cache = self.engine.lock().await;
        if let Err(error) = self.load(&mut cache).await {
            self.close_failed_sockets();
            *cache = None;
            return Err(error);
        }
        let Some(live) = cache.as_mut() else {
            return Ok(());
        };
        let Some(engine) = live.engine.as_mut() else {
            return Ok(());
        };
        let mut host = Host::new(&self.state, &mut live.record);
        let step = match message {
            WebSocketIncomingMessage::String(text) => engine.on_text(conn, &text),
            WebSocketIncomingMessage::Binary(_) => engine.on_binary(conn),
        };
        if let Err(error) = engine.run(&mut host, step).await {
            console_error!("room storage error: {error}");
            *cache = None;
            return Ok(());
        }
        // A socket that joined must still be joined after hibernation.
        if let Some(connection) = engine.connection(conn) {
            if *connection != attachment.connection {
                ws.serialize_attachment(Attachment {
                    conn,
                    connection: connection.clone(),
                    left: false,
                })?;
            }
        }
        if let Err(error) = self.sync_presence(live).await {
            self.close_failed_sockets();
            *cache = None;
            return Err(error);
        }
        Ok(())
    }

    async fn websocket_close(
        &self,
        ws: WebSocket,
        _code: usize,
        _reason: String,
        _was_clean: bool,
    ) -> Result<()> {
        // The runtime replies to Close frames for our compatibility date.
        let Some(attachment) = attachment(&ws).filter(|a| !a.left) else {
            return Ok(());
        };
        let conn = attachment.conn;
        let mut cache = self.engine.lock().await;
        if let Err(error) = self.load(&mut cache).await {
            self.close_failed_sockets();
            *cache = None;
            return Err(error);
        }
        let Some(live) = cache.as_mut() else {
            return Ok(());
        };
        let Some(engine) = live.engine.as_mut() else {
            return Ok(());
        };
        let mut host = Host::new(&self.state, &mut live.record);
        host.sockets.remove(&conn);
        retire(&ws, attachment);
        let step = engine.on_close(conn);
        if let Err(error) = engine.run(&mut host, step).await {
            console_error!("room storage error: {error}");
            *cache = None;
            return Ok(());
        }
        if let Err(error) = self.sync_presence(live).await {
            self.close_failed_sockets();
            *cache = None;
            return Err(error);
        }
        Ok(())
    }

    async fn websocket_error(&self, ws: WebSocket, _error: worker::Error) -> Result<()> {
        <Self as DurableObject>::websocket_close(self, ws, 1011, "error".into(), false).await
    }
}

fn rejected(code: RejectCode) -> Result<Response> {
    let pair = WebSocketPair::new()?;
    pair.server.accept()?;
    pair.server
        .send_with_str(json(&engine::rejected_welcome(code)))?;
    pair.server.close(Some(1000), Some(""))?;
    Response::from_websocket(pair.client)
}
