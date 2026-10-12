//! Browser WebSocket session. The room server owns the game and this client
//! only sends intents and renders the snapshots it receives.

use std::future::Future;

use futures_channel::mpsc::{unbounded, UnboundedSender};

use crate::session::{SessionCommand, SessionEvent, SessionHandle};

#[cfg(target_arch = "wasm32")]
struct SocketGuard {
    socket: web_sys::WebSocket,
    _onopen: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
    _onmessage: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::MessageEvent)>,
    _onclose: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::CloseEvent)>,
    _onerror: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for SocketGuard {
    fn drop(&mut self) {
        self.socket.set_onopen(None);
        self.socket.set_onmessage(None);
        self.socket.set_onclose(None);
        self.socket.set_onerror(None);
        let _ = self.socket.close();
    }
}

/// Open a room socket. The browser's identity travels as the HttpOnly
/// session cookie on the handshake, so there is no token to pass here.
pub fn open_session(
    room_url: String,
    nickname: String,
) -> (SessionHandle, impl Future<Output = ()>) {
    let (commands_tx, commands_rx) = unbounded();
    let (events_tx, events_rx) = unbounded();
    let run = run_session(room_url, nickname, commands_rx, events_tx);
    (
        SessionHandle {
            commands: commands_tx,
            events: events_rx,
        },
        run,
    )
}

#[cfg(target_arch = "wasm32")]
async fn run_session(
    room_url: String,
    nickname: String,
    mut commands: futures_channel::mpsc::UnboundedReceiver<SessionCommand>,
    events: UnboundedSender<SessionEvent>,
) {
    use std::time::Duration;

    use futures::{future, FutureExt, StreamExt};
    use tantrix_proto::{decode, encode, HostMessage, PeerMessage, KEEPALIVE_PING, KEEPALIVE_PONG};

    /// Keeps idle sockets from being dropped by proxies during long turns.
    const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
    use wasm_bindgen::{closure::Closure, JsCast};
    use web_sys::{CloseEvent, Event, MessageEvent, WebSocket};

    enum SocketEvent {
        Open,
        Message(String),
        Closed(String),
        Error,
    }

    let socket = match WebSocket::new(&room_url) {
        Ok(socket) => socket,
        Err(err) => {
            let _ = events.unbounded_send(SessionEvent::Disconnected {
                reason: format!("Could not open the game connection: {err:?}"),
            });
            return;
        }
    };
    let (socket_tx, mut socket_rx) = unbounded::<SocketEvent>();

    let onopen = Closure::<dyn FnMut(Event)>::new({
        let tx = socket_tx.clone();
        move |_| {
            let _ = tx.unbounded_send(SocketEvent::Open);
        }
    });
    socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));

    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new({
        let tx = socket_tx.clone();
        move |event: MessageEvent| {
            if let Some(text) = event.data().as_string() {
                let _ = tx.unbounded_send(SocketEvent::Message(text));
            }
        }
    });
    socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

    let onclose = Closure::<dyn FnMut(CloseEvent)>::new({
        let tx = socket_tx.clone();
        move |event: CloseEvent| {
            let reason = if event.reason().is_empty() {
                "Connection closed.".to_string()
            } else {
                event.reason()
            };
            let _ = tx.unbounded_send(SocketEvent::Closed(reason));
        }
    });
    socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));

    let onerror = Closure::<dyn FnMut(Event)>::new(move |_| {
        let _ = socket_tx.unbounded_send(SocketEvent::Error);
    });
    socket.set_onerror(Some(onerror.as_ref().unchecked_ref()));

    // The guard closes the socket even when Dioxus cancels this future on
    // unmount (the code after the loop would not run in that case).
    let _guard = SocketGuard {
        socket: socket.clone(),
        _onopen: onopen,
        _onmessage: onmessage,
        _onclose: onclose,
        _onerror: onerror,
    };
    let mut timeout = future::Either::<_, future::Pending<()>>::Left(futures_timer::Delay::new(
        Duration::from_secs(20),
    ))
    .fuse();
    let mut welcomed = false;
    let mut keepalive = futures_timer::Delay::new(KEEPALIVE_INTERVAL).fuse();

    loop {
        futures::select! {
            socket_event = socket_rx.next().fuse() => match socket_event {
                Some(SocketEvent::Open) => {
                    let hello = PeerMessage::Hello {
                        nickname: nickname.clone(),
                    };
                    if !send(&socket, &hello) {
                        let _ = events.unbounded_send(SessionEvent::Disconnected {
                            reason: "Could not join the room.".into(),
                        });
                        break;
                    }
                }
                Some(SocketEvent::Message(text)) if text == KEEPALIVE_PONG => {}
                Some(SocketEvent::Message(text)) => {
                    let message: HostMessage = match decode(text.as_bytes()) {
                        Ok(message) => message,
                        Err(err) => {
                            log::warn!("online: invalid server message: {err}");
                            continue;
                        }
                    };
                    match message {
                        HostMessage::Welcome { your_peer_id, lobby, rejected } => {
                            if let Some(code) = rejected {
                                let _ = events.unbounded_send(SessionEvent::Error {
                                    code: Some(code),
                                    message: "The room rejected this connection.".into(),
                                });
                                let _ = events.unbounded_send(SessionEvent::Disconnected {
                                    reason: "The room rejected this connection.".into(),
                                });
                                break;
                            }
                            welcomed = true;
                            timeout = future::Either::Right(future::pending()).fuse();
                            let _ = events.unbounded_send(SessionEvent::Connected { my_peer_id: your_peer_id });
                            let _ = events.unbounded_send(SessionEvent::Lobby(lobby));
                        }
                        HostMessage::LobbyUpdate { lobby } => {
                            let _ = events.unbounded_send(SessionEvent::Lobby(lobby));
                        }
                        HostMessage::GameStarted { snapshot } => {
                            let _ = events.unbounded_send(SessionEvent::GameStarted { snapshot });
                        }
                        HostMessage::StateUpdate { snapshot, last_move } => {
                            let _ = events.unbounded_send(SessionEvent::StateUpdate { snapshot, last_move });
                        }
                        HostMessage::GameEnded { reason, snapshot } => {
                            let _ = events.unbounded_send(SessionEvent::GameEnded { reason, snapshot });
                        }
                        HostMessage::Error { code, message } => {
                            let _ = events.unbounded_send(SessionEvent::Error { code: Some(code), message });
                        }
                    }
                }
                Some(SocketEvent::Closed(reason)) => {
                    let _ = events.unbounded_send(SessionEvent::Disconnected { reason });
                    break;
                }
                Some(SocketEvent::Error) => {
                    log::warn!("online: WebSocket error");
                    // Browsers send a Close event immediately after Error,
                    // which carries the only useful connection detail.
                }
                None => break,
            },
            command = commands.next().fuse() => match command {
                Some(SessionCommand::Leave) | None => break,
                Some(command) if welcomed => {
                    let message = match command {
                        SessionCommand::PickColor(color) => PeerMessage::PickColor { color },
                        SessionCommand::ReplaceSeat(color) => PeerMessage::ReplaceSeat { color },
                        SessionCommand::StartGame => PeerMessage::StartGame,
                        SessionCommand::PlayMove(placement) => PeerMessage::PlayMove { placement },
                        SessionCommand::EndGame => PeerMessage::EndGame,
                        SessionCommand::UpdateNickname(nickname) => PeerMessage::UpdateNickname { nickname },
                        SessionCommand::Leave => unreachable!(),
                    };
                    if !send(&socket, &message) {
                        let _ = events.unbounded_send(SessionEvent::Disconnected {
                            reason: "Could not send to the game server.".into(),
                        });
                        break;
                    }
                }
                Some(_) => {}
            },
            _ = keepalive => {
                if welcomed {
                    let _ = socket.send_with_str(KEEPALIVE_PING);
                }
                keepalive = futures_timer::Delay::new(KEEPALIVE_INTERVAL).fuse();
            },
            _ = timeout => {
                if !welcomed {
                    let _ = events.unbounded_send(SessionEvent::ConnectTimeout);
                    break;
                }
            },
        }
    }
    fn send(socket: &WebSocket, message: &PeerMessage) -> bool {
        let bytes = encode(message);
        let Ok(text) = String::from_utf8(bytes) else {
            return false;
        };
        socket.send_with_str(&text).is_ok()
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "native")))]
async fn run_session(
    _room_url: String,
    _nickname: String,
    _commands: futures_channel::mpsc::UnboundedReceiver<SessionCommand>,
    events: UnboundedSender<SessionEvent>,
) {
    let _ = events.unbounded_send(SessionEvent::Disconnected {
        reason: "Online play requires the web client.".into(),
    });
}

#[cfg(all(not(target_arch = "wasm32"), feature = "native"))]
async fn run_session(
    room_url: String,
    nickname: String,
    commands: futures_channel::mpsc::UnboundedReceiver<SessionCommand>,
    events: UnboundedSender<SessionEvent>,
) {
    crate::native::run_native_session(room_url, nickname, None, commands, events).await;
}
