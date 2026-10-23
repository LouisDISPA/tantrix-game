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
    // Include background/OS suspension in outstanding response deadlines.
    let clock = web_time::SystemTime::now();
    let mut heartbeat = crate::heartbeat::Heartbeat::new(0);

    'session: loop {
        let now = clock.elapsed().unwrap_or_default().as_millis() as u64;
        if heartbeat.expired(now) {
            let _ = events.unbounded_send(SessionEvent::disconnected(
                "The game server stopped responding.".into(),
            ));
            break;
        }
        let keepalive = futures_timer::Delay::new(Duration::from_millis(
            heartbeat.deadline.saturating_sub(now),
        ))
        .fuse();
        futures::pin_mut!(keepalive);
        futures::select_biased! {
            _ = keepalive => {
                if heartbeat.expired(clock.elapsed().unwrap_or_default().as_millis() as u64) {
                    let _ = events.unbounded_send(SessionEvent::disconnected("The game server stopped responding.".into()));
                    break;
                }
                if welcomed {
                    if socket.send_with_str(KEEPALIVE_PING).is_err() {
                        let _ = events.unbounded_send(SessionEvent::disconnected("Connection lost.".into()));
                        break;
                    }
                    heartbeat.ping(clock.elapsed().unwrap_or_default().as_millis() as u64);
                } else {
                    heartbeat = crate::heartbeat::Heartbeat::new(clock.elapsed().unwrap_or_default().as_millis() as u64);
                }
            },
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
                Some(SocketEvent::Message(text)) if text == KEEPALIVE_PONG => {
                    heartbeat.pong(clock.elapsed().unwrap_or_default().as_millis() as u64);
                }
                Some(SocketEvent::Message(text)) => {
                    let message: HostMessage = match decode(text.as_bytes()) {
                        Ok(message) => message,
                        Err(err) => {
                            log::warn!("online: invalid server message: {err}");
                            continue;
                        }
                    };
                    for event in crate::session::incoming_events(message) {
                        if matches!(event, SessionEvent::Connected { .. }) {
                            welcomed = true;
                            timeout = future::Either::Right(future::pending()).fuse();
                        }
                        let disconnected = matches!(event, SessionEvent::Disconnected { .. });
                        let delivered = events.unbounded_send(event).is_ok();
                        if disconnected || !delivered {
                            break 'session;
                        }
                    }
                }
                Some(SocketEvent::Closed(reason)) => {
                    let _ = events.unbounded_send(SessionEvent::disconnected(reason));
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
                    let Some(message) = command.into_peer_message() else {
                        break;
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
