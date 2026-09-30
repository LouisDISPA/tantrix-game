//! Client-side session: thin mirror of the host's authoritative state.

use std::future::Future;

use futures::future::FutureExt;
use futures::{select, StreamExt};
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use log::{debug, info, warn};
use matchbox_socket::{PeerId as MatchboxPeerId, PeerState};
use tantrix_proto::{
    decode, encode, HostMessage, PeerId, PeerMessage, RejectCode, SessionToken, PROTO_VERSION,
};

use crate::session::{ConnectStage, SessionCommand, SessionEvent, SessionHandle, SessionRole};
use crate::socket::{open as open_socket, wait_for_peer_id, PEER_LINK_TIMEOUT, WELCOME_TIMEOUT};

pub fn open_client_session(
    room_url: String,
    nickname: String,
    session_token: SessionToken,
) -> (SessionHandle, impl Future<Output = ()>) {
    let (commands_tx, commands_rx) = futures_channel::mpsc::unbounded();
    let (events_tx, events_rx) = futures_channel::mpsc::unbounded();

    let fut = run_client(room_url, nickname, session_token, commands_rx, events_tx);

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
    session_token: SessionToken,
    mut commands_rx: UnboundedReceiver<SessionCommand>,
    events_tx: UnboundedSender<SessionEvent>,
) {
    let (mut socket, mut channel, mut loop_fut) = open_socket(room_url);

    let Some(my_mb_id) = wait_for_peer_id(
        &mut socket,
        &mut loop_fut,
        &events_tx,
        "signaling closed before id assigned",
    )
    .await
    else {
        return;
    };

    // Wait for the first WebRTC link to open. In a two-player room that is the
    // host; with more guests it may be another guest (every guest offers a
    // link to each newcomer), so `Hello` goes to every link until one of them
    // answers with a `Welcome` — that peer is the host.
    let mut link_deadline = futures_timer::Delay::new(PEER_LINK_TIMEOUT).fuse();
    let first_peer = loop {
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
            update = socket.next().fuse() => match update {
                Some((peer, PeerState::Connected)) => break peer,
                Some((_, PeerState::Disconnected)) => continue,
                None => return,
            },
            cmd = commands_rx.next() => match cmd {
                Some(SessionCommand::Leave) | None => return,
                Some(_) => {} // discard pre-connect intents
            },
            _ = &mut link_deadline => {
                warn!(
                    "client: no WebRTC link opened within {PEER_LINK_TIMEOUT:?} \
                     (signaling ok, ICE/SDP exchange stalled or nobody is hosting)"
                );
                let _ = events_tx.unbounded_send(SessionEvent::ConnectTimeout {
                    stage: ConnectStage::PeerLink,
                });
                return;
            }
        }
    };
    info!("client: first WebRTC link open (peer {first_peer}), sending Hello");

    let hello = encode(&PeerMessage::Hello {
        nickname,
        proto_version: PROTO_VERSION,
        session_token: session_token.clone(),
    });
    let _ = channel.try_send(hello.clone().into_boxed_slice(), first_peer);

    let mut welcome_deadline = futures_timer::Delay::new(WELCOME_TIMEOUT).fuse();
    let mut welcomed = false;
    // The peer that answered our `Hello` with a `Welcome`; only it is trusted.
    let mut host_mb_id: Option<MatchboxPeerId> = None;
    // The wire PeerId the host assigned to us — populated on Welcome and
    // used to recognise `Kicked { target: self }` broadcasts.
    let mut my_wire_id: Option<PeerId> = None;

    loop {
        select! {
            cmd = commands_rx.next() => match cmd {
                Some(SessionCommand::Leave) | None => break,
                Some(other) => {
                    if let Some(host) = host_mb_id {
                        let wire = encode(&intent_from_command(other));
                        let _ = channel.try_send(wire.into_boxed_slice(), host);
                    }
                }
            },
            packet = channel.next().fuse() => match packet {
                Some((peer, bytes)) => {
                    if host_mb_id.is_some_and(|host| host != peer) {
                        continue;
                    }
                    let msg: HostMessage = match decode(&bytes) {
                        Ok(m) => m,
                        Err(e) => {
                            // Other guests' `Hello`s are not `HostMessage`s.
                            debug!("client: ignoring undecodable message from {peer}: {e}");
                            continue;
                        }
                    };
                    if host_mb_id.is_none() {
                        if !matches!(msg, HostMessage::Welcome { .. }) {
                            continue;
                        }
                        info!("client: Welcome from host {peer}");
                        host_mb_id = Some(peer);
                    }
                    if handle_host_message(
                        msg,
                        &events_tx,
                        &mut welcomed,
                        &my_mb_id,
                        &peer,
                        &mut my_wire_id,
                    ) {
                        channel.close();
                        socket.close();
                        return;
                    }
                }
                None => break,
            },
            update = socket.next().fuse() => match update {
                Some((peer, PeerState::Connected)) if host_mb_id.is_none() => {
                    info!("client: another WebRTC link open (peer {peer}), sending Hello");
                    let _ = channel.try_send(hello.clone().into_boxed_slice(), peer);
                }
                Some((peer, PeerState::Disconnected)) if host_mb_id == Some(peer) => {
                    warn!("client: the host disconnected");
                    let _ = events_tx.unbounded_send(SessionEvent::Disconnected {
                        reason: "The host left the room.".into(),
                    });
                    break;
                }
                Some(_) => {}
                None => break,
            },
            _ = &mut welcome_deadline => {
                if !welcomed {
                    warn!(
                        "client: WebRTC link is open but the host sent no Welcome within \
                         {WELCOME_TIMEOUT:?}"
                    );
                    let _ = events_tx.unbounded_send(SessionEvent::ConnectTimeout {
                        stage: ConnectStage::Welcome,
                    });
                    break;
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
        }
    }

    channel.close();
    socket.close();
}

fn intent_from_command(cmd: SessionCommand) -> PeerMessage {
    match cmd {
        SessionCommand::PickColor(c) => PeerMessage::PickColor { color: c },
        SessionCommand::StartGame => PeerMessage::StartGame,
        SessionCommand::PlayMove(m) => PeerMessage::PlayMove { m },
        SessionCommand::SkipTurn => PeerMessage::SkipTurn,
        SessionCommand::UpdateNickname(n) => PeerMessage::UpdateNickname { nickname: n },
        SessionCommand::Kick(target) => PeerMessage::Kick { target },
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
    my_wire_id: &mut Option<PeerId>,
) -> bool {
    match msg {
        HostMessage::Welcome {
            your_peer_id,
            host_peer_id,
            lobby,
            rejected,
        } => {
            if let Some(code) = rejected {
                let reason = match code {
                    RejectCode::Kicked => "You were removed from the room.".into(),
                    RejectCode::IncompatibleVersion => {
                        "The host is running a different game version.".into()
                    }
                    _ => "rejected by host".into(),
                };
                let _ = events_tx.unbounded_send(SessionEvent::Error {
                    code: Some(code),
                    message: "host rejected the connection".into(),
                });
                let _ = events_tx.unbounded_send(SessionEvent::Disconnected { reason });
                return true;
            }
            *my_wire_id = Some(your_peer_id.clone());
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
            ensure_welcomed(welcomed, events_tx, my_id, host_id, my_wire_id);
            let _ = events_tx.unbounded_send(SessionEvent::Lobby(lobby));
            false
        }
        HostMessage::GameStarted { snapshot } => {
            ensure_welcomed(welcomed, events_tx, my_id, host_id, my_wire_id);
            let _ = events_tx.unbounded_send(SessionEvent::GameStarted { snapshot });
            false
        }
        HostMessage::StateUpdate {
            snapshot,
            last_move,
        } => {
            ensure_welcomed(welcomed, events_tx, my_id, host_id, my_wire_id);
            let _ = events_tx.unbounded_send(SessionEvent::StateUpdate {
                snapshot,
                last_move,
            });
            false
        }
        HostMessage::GameEnded { reason, snapshot } => {
            ensure_welcomed(welcomed, events_tx, my_id, host_id, my_wire_id);
            let _ = events_tx.unbounded_send(SessionEvent::GameEnded { reason, snapshot });
            true
        }
        HostMessage::Kicked { target, reason } => {
            if my_wire_id.as_ref() == Some(&target) {
                let _ = events_tx.unbounded_send(SessionEvent::Error {
                    code: Some(RejectCode::Kicked),
                    message: reason.clone(),
                });
                let _ = events_tx.unbounded_send(SessionEvent::Disconnected {
                    reason: format!("Removed from the room: {reason}"),
                });
                true
            } else {
                // Another participant was kicked — host will follow up with
                // a fresh LobbyUpdate / StateUpdate, so nothing else to do
                // here.
                false
            }
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
    my_wire_id: &mut Option<PeerId>,
) {
    if !*welcomed {
        *welcomed = true;
        let fallback = PeerId(my_id.to_string());
        let assigned = my_wire_id.clone().unwrap_or_else(|| fallback.clone());
        if my_wire_id.is_none() {
            *my_wire_id = Some(assigned.clone());
        }
        let _ = events_tx.unbounded_send(SessionEvent::Connected {
            role: SessionRole::Client,
            my_peer_id: assigned,
            host_peer_id: PeerId(host_id.to_string()),
        });
    }
}
