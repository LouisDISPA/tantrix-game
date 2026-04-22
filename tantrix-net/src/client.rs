//! Client-side session: thin mirror of the host's authoritative state.

use std::future::Future;
use std::time::Duration;

use futures::StreamExt;
use futures::future::FutureExt;
use futures::select;
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use log::warn;
use matchbox_socket::{ChannelConfig, PeerId as MatchboxPeerId, PeerState, WebRtcSocket, WebRtcSocketBuilder};
use tantrix_proto::{
    HostMessage, PeerId, PeerMessage, decode, encode, PROTO_VERSION,
};

use crate::session::{SessionCommand, SessionEvent, SessionHandle, SessionRole};

pub fn open_client_session(
    room_url: String,
    nickname: String,
) -> (SessionHandle, impl Future<Output = ()>) {
    let (commands_tx, commands_rx) = futures_channel::mpsc::unbounded();
    let (events_tx, events_rx) = futures_channel::mpsc::unbounded();

    let fut = run_client(room_url, nickname, commands_rx, events_tx);

    (
        SessionHandle {
            commands: commands_tx,
            events: events_rx,
        },
        fut,
    )
}

async fn run_client(
    room_url: String,
    nickname: String,
    mut commands_rx: UnboundedReceiver<SessionCommand>,
    events_tx: UnboundedSender<SessionEvent>,
) {
    let (mut socket, loop_fut) = WebRtcSocketBuilder::new(room_url)
        .add_channel(ChannelConfig::reliable())
        .build();
    let mut loop_fut = loop_fut.fuse();

    // Wait for our own peer id.
    let my_mb_id = loop {
        if let Some(id) = socket.id() {
            break id;
        }
        select! {
            res = &mut loop_fut => {
                let _ = events_tx.unbounded_send(SessionEvent::Disconnected {
                    reason: match res {
                        Ok(()) => "signaling closed before id assigned".into(),
                        Err(e) => format!("signaling error: {e}"),
                    },
                });
                return;
            }
            _ = futures_timer::Delay::new(Duration::from_millis(50)).fuse() => {}
        }
    };

    // Wait for the first peer in the room — that's the host.
    let host_mb_id = loop {
        if let Some(id) = first_connected_peer(&mut socket) {
            break id;
        }
        select! {
            res = &mut loop_fut => {
                let _ = events_tx.unbounded_send(SessionEvent::Disconnected {
                    reason: match res {
                        Ok(()) => "no host in room".into(),
                        Err(e) => format!("signaling error: {e}"),
                    },
                });
                return;
            }
            cmd = commands_rx.next() => match cmd {
                Some(SessionCommand::Leave) | None => return,
                Some(_) => {} // discard pre-connect intents
            },
            _ = futures_timer::Delay::new(Duration::from_millis(50)).fuse() => {}
        }
    };

    // Send Hello and wait for Welcome. Keep draining peer state updates in
    // case the host's connection takes an extra tick to stabilise.
    let hello = encode(&PeerMessage::Hello {
        nickname,
        proto_version: PROTO_VERSION,
    });
    let _ = socket
        .channel_mut(0)
        .try_send(hello.into_boxed_slice(), host_mb_id);

    let mut welcomed = false;

    loop {
        for (_peer, ps) in socket.try_update_peers().unwrap_or_default() {
            // We only care about the host's connection status.
            let _ = ps;
        }

        let inbound = socket.channel_mut(0).receive();
        for (peer, packet) in inbound {
            if peer != host_mb_id {
                // Ignore traffic from other peers — this is a host-auth
                // session, they have nothing to tell us.
                continue;
            }
            let msg: HostMessage = match decode(&packet) {
                Ok(m) => m,
                Err(e) => {
                    warn!("client: bad host message: {e}");
                    continue;
                }
            };
            if handle_host_message(msg, &events_tx, &mut welcomed, &my_mb_id, &host_mb_id) {
                // Session ended normally.
                socket.close();
                return;
            }
        }

        select! {
            cmd = commands_rx.next() => match cmd {
                Some(SessionCommand::Leave) | None => break,
                Some(other) => {
                    let wire = encode(&intent_from_command(other));
                    let _ = socket
                        .channel_mut(0)
                        .try_send(wire.into_boxed_slice(), host_mb_id);
                }
            },
            res = &mut loop_fut => {
                let _ = events_tx.unbounded_send(SessionEvent::Disconnected {
                    reason: match res {
                        Ok(()) => "signaling closed".into(),
                        Err(e) => format!("signaling error: {e}"),
                    },
                });
                break;
            }
            _ = futures_timer::Delay::new(Duration::from_millis(50)).fuse() => {}
        }
    }

    socket.close();
}

fn first_connected_peer(socket: &mut WebRtcSocket) -> Option<MatchboxPeerId> {
    for (peer, state) in socket.try_update_peers().unwrap_or_default() {
        if matches!(state, PeerState::Connected) {
            return Some(peer);
        }
    }
    socket.connected_peers().next()
}

fn intent_from_command(cmd: SessionCommand) -> PeerMessage {
    match cmd {
        SessionCommand::PickColor(c) => PeerMessage::PickColor { color: c },
        SessionCommand::StartGame => PeerMessage::StartGame,
        SessionCommand::PlayMove(m) => PeerMessage::PlayMove { m },
        SessionCommand::SkipTurn => PeerMessage::SkipTurn,
        SessionCommand::Leave => PeerMessage::RequestSnapshot, // never dispatched
    }
}

/// Translate an inbound `HostMessage` into a `SessionEvent`. Returns `true`
/// once the message flow terminates (game ended, rejected welcome, etc.)
/// so the caller can tear down the socket.
fn handle_host_message(
    msg: HostMessage,
    events_tx: &UnboundedSender<SessionEvent>,
    welcomed: &mut bool,
    my_id: &MatchboxPeerId,
    host_id: &MatchboxPeerId,
) -> bool {
    match msg {
        HostMessage::Welcome {
            your_peer_id,
            host_peer_id,
            lobby,
            rejected,
        } => {
            if let Some(code) = rejected {
                let _ = events_tx.unbounded_send(SessionEvent::Error {
                    code: Some(code),
                    message: "host rejected the connection".into(),
                });
                let _ = events_tx.unbounded_send(SessionEvent::Disconnected {
                    reason: "rejected by host".into(),
                });
                return true;
            }
            if !*welcomed {
                *welcomed = true;
                let _ = events_tx.unbounded_send(SessionEvent::Connected {
                    role: SessionRole::Client,
                    my_peer_id: your_peer_id,
                    host_peer_id,
                });
            }
            let _ = events_tx.unbounded_send(SessionEvent::Lobby(lobby));
            false
        }
        HostMessage::LobbyUpdate { lobby } => {
            ensure_welcomed(welcomed, events_tx, my_id, host_id);
            let _ = events_tx.unbounded_send(SessionEvent::Lobby(lobby));
            false
        }
        HostMessage::GameStarted { snapshot } => {
            ensure_welcomed(welcomed, events_tx, my_id, host_id);
            let _ = events_tx.unbounded_send(SessionEvent::GameStarted { snapshot });
            false
        }
        HostMessage::StateUpdate { snapshot, last_move } => {
            ensure_welcomed(welcomed, events_tx, my_id, host_id);
            let _ = events_tx.unbounded_send(SessionEvent::StateUpdate { snapshot, last_move });
            false
        }
        HostMessage::GameEnded { reason, snapshot } => {
            ensure_welcomed(welcomed, events_tx, my_id, host_id);
            let _ = events_tx.unbounded_send(SessionEvent::GameEnded { reason, snapshot });
            true
        }
        HostMessage::Error { code, message } => {
            let _ = events_tx.unbounded_send(SessionEvent::Error {
                code: Some(code),
                message,
            });
            false
        }
    }
}

fn ensure_welcomed(
    welcomed: &mut bool,
    events_tx: &UnboundedSender<SessionEvent>,
    my_id: &MatchboxPeerId,
    host_id: &MatchboxPeerId,
) {
    if !*welcomed {
        *welcomed = true;
        let _ = events_tx.unbounded_send(SessionEvent::Connected {
            role: SessionRole::Client,
            my_peer_id: PeerId(my_id.to_string()),
            host_peer_id: PeerId(host_id.to_string()),
        });
    }
}
