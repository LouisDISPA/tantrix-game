//! Native WebSocket transport. Uses the same commands and events as the browser.

use crate::session::{SessionCommand, SessionEvent, SessionHandle};
use futures_channel::mpsc::{unbounded, UnboundedSender};
use std::future::Future;

/// Native clients keep their private session cookie in application storage.
pub fn open_native_session(
    room_url: String,
    nickname: String,
    cookie: String,
) -> (SessionHandle, impl Future<Output = ()>) {
    let (commands_tx, commands_rx) = unbounded();
    let (events_tx, events_rx) = unbounded();
    (
        SessionHandle {
            commands: commands_tx,
            events: events_rx,
        },
        run_native_session(room_url, nickname, Some(cookie), commands_rx, events_tx),
    )
}

pub(crate) async fn run_native_session(
    room_url: String,
    nickname: String,
    cookie: Option<String>,
    mut commands: futures_channel::mpsc::UnboundedReceiver<SessionCommand>,
    events: UnboundedSender<SessionEvent>,
) {
    use futures::{SinkExt, StreamExt};
    use std::time::Duration;
    use tantrix_proto::{decode, encode, HostMessage, PeerMessage, KEEPALIVE_PING, KEEPALIVE_PONG};
    // Platform TLS avoids choosing between Rustls providers enabled by other crates.
    use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

    // One deadline covers both the HTTP upgrade and the room's Welcome.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let connection = async {
        let mut request = room_url.as_str().into_client_request()?;
        if let Some(cookie) = cookie {
            request.headers_mut().insert("Cookie", cookie.parse()?);
        }
        connect(request).await
    };
    let result = tokio::select! {
        result = tokio::time::timeout_at(deadline, connection) => result,
        _ = commands.next() => return,
    };
    let mut socket = match result {
        Ok(Ok((socket, _))) => socket,
        Ok(Err(error)) => {
            let _ = events.unbounded_send(SessionEvent::Disconnected {
                reason: format!("Could not connect: {error}"),
            });
            return;
        }
        Err(_) => {
            let _ = events.unbounded_send(SessionEvent::ConnectTimeout);
            return;
        }
    };
    let hello = encode(&PeerMessage::Hello { nickname });
    if socket
        .send(Message::Text(String::from_utf8(hello).unwrap().into()))
        .await
        .is_err()
    {
        let _ = events.unbounded_send(SessionEvent::Disconnected {
            reason: "Could not join the room.".into(),
        });
        return;
    }
    let mut welcomed = false;
    let mut keepalive = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(30),
        Duration::from_secs(30),
    );
    let reason = 'session: loop {
        tokio::select! {
            incoming = socket.next() => {
                let text = match incoming {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(Message::Close(frame))) => break frame.map(|f| f.reason.to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Connection closed.".into()),
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => break format!("Connection lost: {error}"),
                    None => break "Connection closed.".into(),
                };
                if text == KEEPALIVE_PONG { continue; }
                let message: HostMessage = match decode(text.as_bytes()) {
                    Ok(message) => message,
                    Err(_) => continue,
                };
                for event in crate::session::incoming_events(message) {
                    if matches!(event, SessionEvent::Connected { .. }) {
                        welcomed = true;
                    }
                    let disconnected = matches!(event, SessionEvent::Disconnected { .. });
                    let delivered = events.unbounded_send(event).is_ok();
                    if disconnected || !delivered {
                        break 'session String::new();
                    }
                }
            }
            command = commands.next() => {
                let command = match command {
                    Some(SessionCommand::Leave) | None => break String::new(),
                    Some(_) if !welcomed => continue,
                    Some(command) => command,
                };
                let Some(message) = command.into_peer_message() else {
                    break String::new();
                };
                let text = String::from_utf8(encode(&message)).unwrap();
                if socket.send(Message::Text(text.into())).await.is_err() { break "Could not send to the game server.".into(); }
            }
            _ = keepalive.tick(), if welcomed => {
                if socket.send(Message::Text(KEEPALIVE_PING.into())).await.is_err() { break "Connection lost.".into(); }
            }
            _ = tokio::time::sleep_until(deadline), if !welcomed => {
                let _ = events.unbounded_send(SessionEvent::ConnectTimeout);
                break String::new();
            }
        }
    };
    if !reason.is_empty() {
        let _ = events.unbounded_send(SessionEvent::Disconnected { reason });
    }
    // Dropping this future also drops the socket when its component unmounts.
    let _ = tokio::time::timeout(Duration::from_secs(1), socket.close(None)).await;
}

async fn connect(
    request: tokio_tungstenite::tungstenite::handshake::client::Request,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    ),
    tokio_tungstenite::tungstenite::Error,
> {
    #[cfg(not(target_os = "android"))]
    let connection = tokio_tungstenite::connect_async(request).await;
    #[cfg(target_os = "android")]
    let connection = {
        // Select a provider explicitly: Dioxus dependencies can enable both
        // Ring and AWS-LC, making Rustls's automatic selection ambiguous.
        let roots =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = rustls::ClientConfig::builder_with_provider(
            rustls::crypto::ring::default_provider().into(),
        )
        .with_safe_default_protocol_versions()
        .expect("Ring supports the default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
        tokio_tungstenite::connect_async_tls_with_config(
            request,
            None,
            false,
            Some(tokio_tungstenite::Connector::Rustls(config.into())),
        )
        .await
    };
    connection
}
