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
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{client::IntoClientRequest, Message},
    };

    // One deadline covers both the HTTP upgrade and the room's Welcome.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let connection = async {
        let mut request = room_url.as_str().into_client_request()?;
        if let Some(cookie) = cookie {
            request.headers_mut().insert("Cookie", cookie.parse()?);
        }
        connect_async(request).await
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
    let reason = loop {
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
                let event = match message {
                    HostMessage::Welcome { your_peer_id, lobby, rejected } => {
                        if let Some(code) = rejected {
                            let _ = events.unbounded_send(SessionEvent::Error { code: Some(code), message: "The room rejected this connection.".into() });
                            break "The room rejected this connection.".into();
                        }
                        welcomed = true;
                        let _ = events.unbounded_send(SessionEvent::Connected { my_peer_id: your_peer_id });
                        SessionEvent::Lobby(lobby)
                    }
                    HostMessage::LobbyUpdate { lobby } => SessionEvent::Lobby(lobby),
                    HostMessage::GameStarted { snapshot } => SessionEvent::GameStarted { snapshot },
                    HostMessage::StateUpdate { snapshot, last_move } => SessionEvent::StateUpdate { snapshot, last_move },
                    HostMessage::GameEnded { reason, snapshot } => SessionEvent::GameEnded { reason, snapshot },
                    HostMessage::Error { code, message } => SessionEvent::Error { code: Some(code), message },
                };
                if events.unbounded_send(event).is_err() { break String::new(); }
            }
            command = commands.next() => {
                let message = match command {
                    Some(SessionCommand::Leave) | None => break String::new(),
                    Some(_) if !welcomed => continue,
                    Some(SessionCommand::PickColor(color)) => PeerMessage::PickColor { color },
                    Some(SessionCommand::ReplaceSeat(color)) => PeerMessage::ReplaceSeat { color },
                    Some(SessionCommand::StartGame) => PeerMessage::StartGame,
                    Some(SessionCommand::PlayMove(placement)) => PeerMessage::PlayMove { placement },
                    Some(SessionCommand::EndGame) => PeerMessage::EndGame,
                    Some(SessionCommand::UpdateNickname(nickname)) => PeerMessage::UpdateNickname { nickname },
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
