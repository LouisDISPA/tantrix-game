//! Hibernatable, authoritative room Durable Object. A `RoomService`
//! (`tantrix_room::service`) decides what every request, socket event and
//! alarm does; this object owns the sockets, their attachments, keepalive
//! auto-responses and Durable Object storage.

use std::collections::BTreeMap;

use crate::catalog::{self, WorkerCatalog};
use futures::lock::Mutex;
use serde::{Deserialize, Serialize};
use tantrix_proto::{HostMessage, RejectCode, RoomApiError};
use tantrix_room::engine::{self, ConnId, Connection, Effect};
use tantrix_room::handshake::{self, Refusal, SiteOrigin, Upgrade};
use tantrix_room::lifecycle::RoomRecord;
use tantrix_room::room_logic::CompletedMatch;
use tantrix_room::service::{RoomService, ServiceHost};
use tantrix_room::session_cookie;
use worker::{
    durable_object, DurableObject, Env, Request, Response, Result, State, WebSocket,
    WebSocketIncomingMessage, WebSocketPair,
};

const STORAGE_KEY: &str = "record";

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

/// Mark the socket as done, so a rebuilt room leaves it out.
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

/// Accept a socket only to close it with a reason the player reads.
fn closed(reason: &str) -> Result<Response> {
    let pair = WebSocketPair::new()?;
    pair.server.accept()?;
    pair.server.close(Some(1000), Some(reason))?;
    Response::from_websocket(pair.client)
}

#[durable_object]
pub struct Room {
    state: State,
    /// The room while the object is awake. After hibernation it is rebuilt
    /// from storage and the socket attachments.
    service: Mutex<Option<RoomService>>,
    env: Env,
}

/// The object's I/O for its room: storage, invitation codes, and the
/// sockets the room still serves.
struct Host {
    catalog: WorkerCatalog,
    sockets: BTreeMap<ConnId, (WebSocket, Attachment)>,
}

impl Host {
    fn new(state: &State, env: &Env) -> Self {
        let sockets = state
            .get_websockets()
            .into_iter()
            .filter_map(|ws| {
                let attachment = attachment(&ws).filter(|a| !a.left)?;
                Some((attachment.conn, (ws, attachment)))
            })
            .collect();
        Self {
            catalog: WorkerCatalog {
                storage: state.storage(),
                env: env.clone(),
            },
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

impl ServiceHost for Host {
    type Catalog = WorkerCatalog;

    fn catalog(&mut self) -> &mut WorkerCatalog {
        &mut self.catalog
    }

    async fn save_record(
        &mut self,
        record: &RoomRecord,
        archive: Option<CompletedMatch>,
    ) -> std::result::Result<(), RoomApiError> {
        catalog::persist_with_archive(&self.catalog.storage, record, archive)
            .await
            .map_err(catalog::unavailable)
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
    /// Wake the room unless it is awake: read its record and settle it with
    /// the sockets that outlived hibernation. A service that closed (storage
    /// failed, or the room was deleted) is read again. Nothing stored leaves
    /// `cache` empty. On failure every socket is closed.
    async fn load(
        &self,
        cache: &mut Option<RoomService>,
        host: &mut Host,
    ) -> std::result::Result<(), RoomApiError> {
        if cache
            .as_ref()
            .is_some_and(|service| service.closed().is_none())
        {
            return Ok(());
        }
        *cache = None;
        let record = match self.state.storage().get::<RoomRecord>(STORAGE_KEY).await {
            Ok(record) => record,
            Err(error) => {
                let sockets = host.sockets.keys().copied().collect::<Vec<_>>();
                host.deliver(engine::storage_unavailable(sockets));
                return Err(catalog::unavailable(error));
            }
        };
        let Some(record) = record else {
            return Ok(());
        };
        let mut service = RoomService::new(record, host.connections());
        service.load(host, catalog::now_ms()).await?;
        *cache = Some(service);
        Ok(())
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

fn failed(error: RoomApiError) -> worker::Error {
    worker::Error::RustError(error.message().into())
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
            service: Mutex::new(None),
            env,
        }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        let url = req.url()?;
        let websocket = req.headers().get("Upgrade")?.as_deref() == Some("websocket");
        // Refused sockets never touch the room or its storage.
        if websocket {
            let origin = req.headers().get("Origin")?;
            let site = url.origin().ascii_serialization();
            let checked = handshake::check(&Upgrade {
                id: url.path().strip_prefix("/room/").unwrap_or_default(),
                origin: origin.as_deref(),
                site: SiteOrigin::Exact(&site),
                query: url.query(),
            });
            match checked {
                Ok(()) => {}
                Err(refusal) if refusal.closes_socket() => return closed(refusal.message()),
                Err(refusal) => return refuse(refusal),
            }
        }
        let mut cache = self.service.lock().await;
        let mut host = Host::new(&self.state, &self.env);
        if let Err(error) = self.load(&mut cache, &mut host).await {
            return crate::rooms_api::error(error);
        }
        let now = catalog::now_ms();
        if let Some(id) = url.path().strip_prefix("/create/") {
            if !tantrix_proto::is_game_id(id) {
                return crate::rooms_api::error(RoomApiError::InvalidRequest);
            }
            if cache.is_none() {
                let mut service = RoomService::new(RoomRecord::new(id.into(), now), []);
                if let Err(error) = service.load(&mut host, now).await {
                    return crate::rooms_api::error(error);
                }
                *cache = Some(service);
            }
        }
        if url.path().starts_with("/room/") && !websocket {
            return Response::error("expected websocket", 426);
        }
        let Some(service) = cache.as_mut() else {
            if !websocket {
                return crate::rooms_api::error(RoomApiError::NotFound);
            }
            return rejected(RejectCode::RoomNotFound);
        };
        let allocate =
            websocket || url.path().starts_with("/create/") || url.query() == Some("allocate=1");
        let reference = match service.inspect(&mut host, allocate, now).await {
            Ok(reference) => reference,
            Err(error) => {
                return match (websocket, error) {
                    (true, RoomApiError::Expired) => rejected(RejectCode::RoomExpired),
                    (true, RoomApiError::NotFound) => rejected(RejectCode::RoomNotFound),
                    _ => crate::rooms_api::error(error),
                };
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
        host.sockets.insert(conn, (pair.server.clone(), attachment));
        // A failure closes the socket with its reason, which the player reads.
        let _ = service.open(&mut host, conn, token, now).await;
        let mut response = Response::from_websocket(pair.client)?;
        if let Some(cookie) = set_cookie {
            response.headers_mut().set("Set-Cookie", &cookie)?;
        }
        Ok(response)
    }

    async fn alarm(&self) -> Result<Response> {
        let mut cache = self.service.lock().await;
        let mut host = Host::new(&self.state, &self.env);
        // A failed alarm is retried.
        self.load(&mut cache, &mut host).await.map_err(failed)?;
        if let Some(service) = cache.as_mut() {
            match service.inspect(&mut host, false, catalog::now_ms()).await {
                Ok(_) | Err(RoomApiError::Expired | RoomApiError::NotFound) => {}
                Err(error) => return Err(failed(error)),
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
        let mut cache = self.service.lock().await;
        let mut host = Host::new(&self.state, &self.env);
        self.load(&mut cache, &mut host).await.map_err(failed)?;
        let Some(service) = cache.as_mut() else {
            return Ok(());
        };
        let now = catalog::now_ms();
        let _ = match message {
            WebSocketIncomingMessage::String(text) => {
                service.text(&mut host, conn, &text, now).await
            }
            WebSocketIncomingMessage::Binary(_) => service.binary(&mut host, conn, now).await,
        };
        // A socket that joined must still be joined after hibernation.
        let joined = service
            .engine()
            .and_then(|engine| engine.connection(conn))
            .filter(|connection| **connection != attachment.connection);
        if let Some(connection) = joined {
            ws.serialize_attachment(Attachment {
                conn,
                connection: connection.clone(),
                left: false,
            })?;
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
        let mut cache = self.service.lock().await;
        let mut host = Host::new(&self.state, &self.env);
        self.load(&mut cache, &mut host).await.map_err(failed)?;
        let Some(service) = cache.as_mut() else {
            return Ok(());
        };
        host.sockets.remove(&conn);
        retire(&ws, attachment);
        let _ = service.close(&mut host, conn, catalog::now_ms()).await;
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
