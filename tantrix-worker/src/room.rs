//! Hibernatable, authoritative room Durable Object.

use std::collections::HashSet;

use futures::lock::Mutex;
use serde::{Deserialize, Serialize};
use tantrix_proto::{HostMessage, PeerId, PeerMessage, RejectCode};
use worker::{
    durable_object, DurableObject, Env, Request, Response, Result, State, Url, WebSocket,
    WebSocketIncomingMessage, WebSocketPair,
};

use crate::room_logic::RoomData;
use crate::session_cookie::{self, SessionToken};

const STORAGE_KEY: &str = "room";
const MAX_MESSAGE_BYTES: usize = 8 * 1024;

/// Stored on each socket so it survives hibernation. `token` comes from the
/// handshake cookie; `joined` flips once the room accepted the `Hello`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SocketIdentity {
    token: SessionToken,
    joined: bool,
    left: bool,
}

impl SocketIdentity {
    fn active(&self) -> bool {
        self.joined && !self.left
    }
}

#[durable_object(websocket)]
pub struct Room {
    state: State,
    room: Mutex<Option<RoomData>>,
}

impl Room {
    async fn load<'a>(&self, cache: &'a mut Option<RoomData>) -> Result<&'a mut RoomData> {
        if cache.is_none() {
            *cache = self.state.storage().get(STORAGE_KEY).await?;
        }
        cache
            .as_mut()
            .ok_or_else(|| worker::Error::RustError("room does not exist".into()))
    }

    async fn save(&self, room: &RoomData) -> Result<()> {
        self.state.storage().put(STORAGE_KEY, room).await
    }

    fn identity(ws: &WebSocket) -> Option<SocketIdentity> {
        ws.deserialize_attachment().ok().flatten()
    }

    fn connected(&self) -> HashSet<SessionToken> {
        self.state
            .get_websockets()
            .iter()
            .filter_map(Self::identity)
            .filter(SocketIdentity::active)
            .map(|identity| identity.token)
            .collect()
    }

    fn send(ws: &WebSocket, message: &HostMessage) {
        if let Ok(json) = serde_json::to_string(message) {
            let _ = ws.send_with_str(json);
        }
    }

    fn broadcast(&self, message: &HostMessage, except: Option<&WebSocket>) {
        for socket in self.state.get_websockets() {
            if except.is_some_and(|excluded| &socket == excluded) {
                continue;
            }
            if Self::identity(&socket).is_some_and(|identity| identity.active()) {
                Self::send(&socket, message);
            }
        }
    }

    fn reject(ws: &WebSocket, code: RejectCode, message: &str) {
        Self::send(
            ws,
            &HostMessage::Error {
                code,
                message: message.into(),
            },
        );
    }

    fn reject_welcome(ws: &WebSocket, code: RejectCode) {
        Self::send(
            ws,
            &HostMessage::Welcome {
                your_peer_id: PeerId::new(""),
                lobby: RoomData::default().lobby(&HashSet::new()),
                rejected: Some(code),
            },
        );
        let _ = ws.close(Some(1000), Some(""));
    }

    /// Browsers always send `Origin`; refusing other sites stops a foreign
    /// page from opening a socket that rides on our player's cookie.
    fn same_origin(req: &Request, url: &Url) -> Result<bool> {
        Ok(match req.headers().get("Origin")? {
            Some(origin) => origin == url.origin().ascii_serialization(),
            None => true,
        })
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
            room: Mutex::new(None),
        }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        if req.headers().get("Upgrade")?.as_deref() != Some("websocket") {
            return Response::error("expected websocket", 426);
        }

        let url = req.url()?;
        if !Self::same_origin(&req, &url)? {
            return Response::error("cross-origin room connection", 403);
        }
        // Pages set the cookie on first visit; mint one here only if the
        // browser arrived without it, and hand it back on the upgrade.
        let cookie = session_cookie::read(req.headers().get("Cookie")?.as_deref());
        let minted = cookie.is_none();
        let token = cookie.unwrap_or_else(SessionToken::generate);
        let create = url
            .query_pairs()
            .any(|(name, value)| name == "create" && value == "1");
        let mut cache = self.room.lock().await;
        if cache.is_none() {
            *cache = self.state.storage().get(STORAGE_KEY).await?;
        }
        let missing = cache.is_none();
        if missing && create {
            let fresh = RoomData::default();
            self.save(&fresh).await?;
            *cache = Some(fresh);
        }
        if !missing && create {
            return Response::error("room code already exists", 409);
        }
        drop(cache);

        let pair = WebSocketPair::new()?;
        self.state.accept_web_socket(&pair.server);
        pair.server.serialize_attachment(SocketIdentity {
            token: token.clone(),
            joined: false,
            left: false,
        })?;
        if missing && !create {
            Self::reject_welcome(&pair.server, RejectCode::RoomNotFound);
        }
        let response = Response::from_websocket(pair.client)?;
        if minted {
            let cookie = session_cookie::set_cookie(&token, url.scheme() == "https");
            response.headers().set("Set-Cookie", &cookie)?;
        }
        Ok(response)
    }

    async fn websocket_message(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        let WebSocketIncomingMessage::String(text) = message else {
            Self::reject(&ws, RejectCode::InvalidMove, "text messages only");
            return Ok(());
        };
        if text.len() > MAX_MESSAGE_BYTES {
            Self::reject(&ws, RejectCode::InvalidMove, "message too large");
            return Ok(());
        }
        let Ok(intent) = serde_json::from_str::<PeerMessage>(&text) else {
            Self::reject(&ws, RejectCode::InvalidMove, "invalid message");
            return Ok(());
        };

        let mut cache = self.room.lock().await;
        let room = match self.load(&mut cache).await {
            Ok(room) => room,
            Err(_) => {
                Self::reject_welcome(&ws, RejectCode::RoomNotFound);
                return Ok(());
            }
        };

        let Some(identity) = Self::identity(&ws) else {
            Self::reject_welcome(&ws, RejectCode::InvalidMove);
            return Ok(());
        };
        if identity.left {
            return Ok(());
        }

        if let PeerMessage::Hello { nickname } = intent {
            if identity.joined {
                Self::reject(&ws, RejectCode::InvalidMove, "already joined");
                return Ok(());
            }
            let session_token = identity.token;
            let existed = room.player(&session_token).is_some();
            let public_id = PeerId::new(uuid::Uuid::new_v4().to_string());
            let peer_id = match room.hello(session_token.clone(), nickname, public_id) {
                Ok(id) => id,
                Err(code) => {
                    Self::reject_welcome(&ws, code);
                    return Ok(());
                }
            };
            if !existed {
                self.save(room).await?;
            }
            // Same cookie = same player: a reload, reconnect, or second tab
            // takes over, and any older socket of that player is retired.
            for old in self.state.get_websockets() {
                if old != ws
                    && Self::identity(&old)
                        .is_some_and(|other| other.active() && other.token == session_token)
                {
                    let _ = old.serialize_attachment(SocketIdentity {
                        token: session_token.clone(),
                        joined: true,
                        left: true,
                    });
                    let _ = old.close(Some(1000), Some("This room was opened in another tab."));
                }
            }
            ws.serialize_attachment(SocketIdentity {
                token: session_token,
                joined: true,
                left: false,
            })?;
            let connected = self.connected();
            Self::send(
                &ws,
                &HostMessage::Welcome {
                    your_peer_id: peer_id,
                    lobby: room.lobby(&connected),
                    rejected: None,
                },
            );
            if room.snapshot(&connected).is_some() {
                Self::send(&ws, &room.current_message(&connected));
            }
            self.broadcast(&room.roster_message(&connected), Some(&ws));
            return Ok(());
        }

        if !identity.joined {
            Self::reject(&ws, RejectCode::InvalidMove, "say hello first");
            return Ok(());
        }
        let connected = self.connected();
        match room.apply(&identity.token, intent, &connected) {
            Ok(change) => {
                if change.persist {
                    self.save(room).await?;
                    self.broadcast(&change.message, None);
                } else {
                    Self::send(&ws, &change.message);
                }
            }
            Err((code, message)) => Self::reject(&ws, code, &message),
        }
        Ok(())
    }

    async fn websocket_close(
        &self,
        ws: WebSocket,
        code: usize,
        _reason: String,
        _was_clean: bool,
    ) -> Result<()> {
        let Some(identity) = Self::identity(&ws) else {
            return Ok(());
        };
        if !identity.active() {
            return Ok(());
        }
        ws.serialize_attachment(SocketIdentity {
            token: identity.token.clone(),
            joined: true,
            left: true,
        })?;
        let mut cache = self.room.lock().await;
        let room = self.load(&mut cache).await?;
        let changed = room.disconnect(&identity.token);
        if changed {
            self.save(room).await?;
        }
        let connected = self.connected();
        self.broadcast(&room.roster_message(&connected), Some(&ws));
        let _ = code; // The runtime replies to Close frames for our compatibility date.
        Ok(())
    }

    async fn websocket_error(&self, ws: WebSocket, _error: worker::Error) -> Result<()> {
        <Self as DurableObject>::websocket_close(self, ws, 1011, "error".into(), false).await
    }
}
