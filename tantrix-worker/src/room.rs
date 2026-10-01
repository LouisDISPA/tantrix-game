//! Hibernatable, authoritative room Durable Object. A `RoomEngine`
//! (`tantrix_room::engine`) decides what every socket event does; this
//! object owns the sockets, their attachments, keepalive auto-responses and
//! Durable Object storage.

use std::collections::BTreeMap;
use std::future::Future;

use futures::lock::Mutex;
use serde::{Deserialize, Serialize};
use tantrix_proto::{HostMessage, RejectCode};
use tantrix_room::engine::{self, ConnId, Connection, Effect, RoomEngine, RoomHost};
use tantrix_room::handshake::{self, Refusal};
use tantrix_room::room_logic::RoomData;
use tantrix_room::session_cookie;
use worker::{
    console_error, durable_object, DurableObject, Env, Request, Response, Result, State, Storage,
    WebSocket, WebSocketIncomingMessage, WebSocketPair,
};

const STORAGE_KEY: &str = "room";

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

#[durable_object(websocket)]
pub struct Room {
    state: State,
    /// The room and its sockets while the object is awake. After
    /// hibernation it is rebuilt from storage and the socket attachments.
    engine: Mutex<Option<RoomEngine>>,
}

/// The object's I/O for the engine: storage, and the sockets the room
/// still serves.
struct Host {
    storage: Storage,
    sockets: BTreeMap<ConnId, (WebSocket, Attachment)>,
}

impl Host {
    fn new(state: &State) -> Self {
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

impl RoomHost for Host {
    type Error = worker::Error;

    fn save(&mut self, room: &RoomData) -> impl Future<Output = Result<()>> {
        self.storage.put(STORAGE_KEY, room)
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
    /// The room's engine, loaded from storage if the object just woke up.
    /// `Ok(None)`: no room has this code. On a storage error every socket
    /// has been closed.
    async fn engine<'a>(
        &self,
        cache: &'a mut Option<RoomEngine>,
        host: &mut Host,
    ) -> Result<Option<&'a mut RoomEngine>> {
        if cache.is_none() {
            let stored = match host.storage.get::<RoomData>(STORAGE_KEY).await {
                Ok(stored) => stored,
                Err(error) => {
                    let sockets: Vec<_> = host.sockets.keys().copied().collect();
                    host.deliver(engine::storage_unavailable(sockets));
                    return Err(error);
                }
            };
            let Some(room) = stored else {
                return Ok(None);
            };
            let mut engine = RoomEngine::new(room, host.connections());
            let loaded = engine.on_load();
            engine.run(host, loaded).await?;
            *cache = Some(engine);
        }
        Ok(cache.as_mut())
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
    fn new(state: State, _env: Env) -> Self {
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
        }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        if req.headers().get("Upgrade")?.as_deref() != Some("websocket") {
            return Response::error("expected websocket", 426);
        }
        let url = req.url()?;
        let code = url.path().strip_prefix("/room/").unwrap_or_default();
        let origin = req.headers().get("Origin")?;
        let expected_origin = url.origin().ascii_serialization();
        if let Err(refusal) = handshake::check(code, origin.as_deref(), Some(&expected_origin)) {
            return refuse(refusal);
        }
        let cookie = req.headers().get("Cookie")?;
        let (token, set_cookie) =
            session_cookie::identify(cookie.as_deref(), url.scheme() == "https");
        let create = handshake::wants_create(url.query());

        let mut cache = self.engine.lock().await;
        let mut host = Host::new(&self.state);
        let exists = match self.engine(&mut cache, &mut host).await {
            Ok(engine) => engine.is_some(),
            Err(error) => {
                console_error!("room storage error: {error}");
                return refuse(Refusal::StorageUnavailable);
            }
        };
        if create {
            if exists {
                return refuse(Refusal::CodeTaken);
            }
            let room = RoomData::default();
            if let Err(error) = host.save(&room).await {
                console_error!("room storage error: {error}");
                return refuse(Refusal::StorageUnavailable);
            }
            *cache = Some(RoomEngine::new(room, host.connections()));
        }

        let pair = WebSocketPair::new()?;
        self.state.accept_web_socket(&pair.server);
        let conn = self.next_conn();
        let attachment = Attachment {
            conn,
            connection: Connection {
                token: token.clone(),
                joined: false,
            },
            left: false,
        };
        match cache.as_mut() {
            Some(engine) => {
                pair.server.serialize_attachment(&attachment)?;
                host.sockets.insert(conn, (pair.server.clone(), attachment));
                let opened = engine.on_open(conn, token);
                // Nothing to save: a new socket only joins the room's list.
                let _ = engine.run(&mut host, opened).await;
            }
            None => {
                retire(&pair.server, attachment);
                let _ = pair
                    .server
                    .send_with_str(json(&engine::rejected_welcome(RejectCode::RoomNotFound)));
                let _ = pair.server.close(Some(1000), Some(""));
            }
        }
        let response = Response::from_websocket(pair.client)?;
        if let Some(cookie) = set_cookie {
            response.headers().set("Set-Cookie", &cookie)?;
        }
        Ok(response)
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
        let mut host = Host::new(&self.state);
        let engine = match self.engine(&mut cache, &mut host).await {
            Ok(Some(engine)) => engine,
            Ok(None) => {
                retire(&ws, attachment);
                let _ = ws.send_with_str(json(&engine::rejected_welcome(RejectCode::RoomNotFound)));
                let _ = ws.close(Some(1000), Some(""));
                return Ok(());
            }
            Err(error) => {
                console_error!("room storage error: {error}");
                return Ok(());
            }
        };
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
        let mut host = Host::new(&self.state);
        // Load before retiring the socket, so a rebuilt engine still has it.
        let loaded = self.engine(&mut cache, &mut host).await;
        host.sockets.remove(&conn);
        retire(&ws, attachment);
        let engine = match loaded {
            Ok(Some(engine)) => engine,
            Ok(None) => return Ok(()),
            Err(error) => {
                console_error!("room storage error: {error}");
                return Ok(());
            }
        };
        let step = engine.on_close(conn);
        if let Err(error) = engine.run(&mut host, step).await {
            console_error!("room storage error: {error}");
            *cache = None;
        }
        Ok(())
    }

    async fn websocket_error(&self, ws: WebSocket, _error: worker::Error) -> Result<()> {
        <Self as DurableObject>::websocket_close(self, ws, 1011, "error".into(), false).await
    }
}
