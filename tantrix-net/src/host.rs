//! Host-side session: owns the authoritative `Game` and the lobby.

use std::collections::HashMap;
use std::future::Future;

use enum_map::EnumMap;
use futures::future::FutureExt;
use futures::{StreamExt, select};
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use log::{debug, warn};
use matchbox_socket::{PeerId as MatchboxPeerId, PeerState, WebRtcChannel};
use tantrix_game::{Color, Game, Hand, Player};
use tantrix_proto::{
    EndReason, GameSnapshot, HostMessage, LobbyState, PeerId, PeerMessage, RejectCode, Seat,
    decode, encode, PROTO_VERSION,
};

use crate::session::{SessionCommand, SessionEvent, SessionHandle, SessionRole};
use crate::socket::{open as open_socket, wait_for_peer_id};

pub fn open_host_session(
    room_url: String,
    nickname: String,
) -> (SessionHandle, impl Future<Output = ()>) {
    let (commands_tx, commands_rx) = futures_channel::mpsc::unbounded();
    let (events_tx, events_rx) = futures_channel::mpsc::unbounded();

    let fut = run_host(room_url, nickname, commands_rx, events_tx);

    (
        SessionHandle {
            commands: commands_tx,
            events: events_rx,
        },
        fut,
    )
}

#[derive(Clone)]
struct PeerInfo {
    nickname: String,
}

struct HostState {
    my_mb_id: MatchboxPeerId,
    /// All peers (including the host) known by their matchbox id.
    peers: HashMap<MatchboxPeerId, PeerInfo>,
    game: Option<Game>,
    seats: EnumMap<Color, Option<SeatBinding>>,
    ended: bool,
}

#[derive(Clone)]
struct SeatBinding {
    mb_id: MatchboxPeerId,
    nickname: String,
}

impl SeatBinding {
    fn to_wire(&self) -> Seat {
        Seat {
            peer_id: mb_to_wire(&self.mb_id),
            nickname: self.nickname.clone(),
        }
    }
}

impl HostState {
    fn new(my_mb_id: MatchboxPeerId, nickname: String) -> Self {
        let mut peers = HashMap::new();
        peers.insert(my_mb_id, PeerInfo { nickname });
        Self {
            my_mb_id,
            peers,
            game: None,
            seats: EnumMap::default(),
            ended: false,
        }
    }

    fn wire_seats(&self) -> EnumMap<Color, Option<Seat>> {
        EnumMap::from_fn(|c| self.seats[c].as_ref().map(|s| s.to_wire()))
    }

    fn lobby_state(&self) -> LobbyState {
        let unseated: Vec<Seat> = self
            .peers
            .iter()
            .filter(|(mb, _)| {
                self.seats
                    .values()
                    .all(|s| s.as_ref().is_none_or(|seat| seat.mb_id != **mb))
            })
            .map(|(mb, info)| Seat {
                peer_id: mb_to_wire(mb),
                nickname: info.nickname.clone(),
            })
            .collect();

        LobbyState {
            host: mb_to_wire(&self.my_mb_id),
            seats: self.wire_seats(),
            unseated,
        }
    }

    fn snapshot(&self) -> Option<GameSnapshot> {
        let game = self.game.as_ref()?;
        let playing_colors: Vec<Color> = game.playing_colors().to_vec();
        let mut hands: EnumMap<Color, Hand> = EnumMap::default();
        for &c in &playing_colors {
            hands[c] = game.get_hand(c).clone();
        }
        Some(GameSnapshot {
            board: game.board().clone(),
            hands,
            bag_count: game.bag_count(),
            current_player: game.currently_playing(),
            playing_colors,
            seats: self.wire_seats(),
        })
    }

    fn seated_color(&self, peer: &MatchboxPeerId) -> Option<Color> {
        for (color, slot) in self.seats.iter() {
            if let Some(seat) = slot {
                if &seat.mb_id == peer {
                    return Some(color);
                }
            }
        }
        None
    }
}

fn mb_to_wire(id: &MatchboxPeerId) -> PeerId {
    PeerId(id.to_string())
}

async fn run_host(
    room_url: String,
    nickname: String,
    mut commands_rx: UnboundedReceiver<SessionCommand>,
    events_tx: UnboundedSender<SessionEvent>,
) {
    let (mut socket, mut channel, mut loop_fut) = open_socket(room_url);

    let Some(my_mb_id) = wait_for_peer_id(
        &mut socket,
        &mut loop_fut,
        &events_tx,
        "signaling closed before peer id was assigned",
    )
    .await
    else {
        return;
    };

    let my_wire_id = mb_to_wire(&my_mb_id);
    let mut state = HostState::new(my_mb_id, nickname);

    let _ = events_tx.unbounded_send(SessionEvent::Connected {
        role: SessionRole::Host,
        my_peer_id: my_wire_id.clone(),
        host_peer_id: my_wire_id,
    });
    let _ = events_tx.unbounded_send(SessionEvent::Lobby(state.lobby_state()));

    // Main loop — event-driven, no polling. Each select arm reacts to the
    // single event source that actually changed.
    while !state.ended {
        select! {
            cmd = commands_rx.next() => match cmd {
                Some(SessionCommand::Leave) | None => break,
                Some(other) => {
                    let local = intent_from_command(other);
                    let me = state.my_mb_id;
                    handle_intent(&mut state, me, local, &events_tx, &mut channel);
                }
            },
            packet = channel.next().fuse() => match packet {
                Some((peer, bytes)) => {
                    let msg: PeerMessage = match decode(&bytes) {
                        Ok(m) => m,
                        Err(e) => {
                            warn!("host: bad peer message from {peer}: {e}");
                            continue;
                        }
                    };
                    handle_intent(&mut state, peer, msg, &events_tx, &mut channel);
                }
                None => break,
            },
            peer_update = socket.next().fuse() => match peer_update {
                Some((peer, PeerState::Connected)) => debug!("host: peer {peer} connected"),
                Some((peer, PeerState::Disconnected)) => {
                    debug!("host: peer {peer} disconnected");
                    handle_peer_disconnect(&mut state, &peer, &events_tx, &mut channel);
                }
                None => break,
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
        SessionCommand::Leave => PeerMessage::RequestSnapshot, // never actually dispatched
    }
}

fn handle_peer_disconnect(
    state: &mut HostState,
    peer: &MatchboxPeerId,
    events_tx: &UnboundedSender<SessionEvent>,
    channel: &mut WebRtcChannel,
) {
    state.peers.remove(peer);
    let seated = state.seated_color(peer);
    for slot in state.seats.values_mut() {
        if let Some(seat) = slot {
            if &seat.mb_id == peer {
                *slot = None;
            }
        }
    }

    if state.game.is_some() {
        if let Some(color) = seated {
            let snap = state.snapshot().expect("game in progress");
            let reason = EndReason::PlayerDisconnected { color };
            publish_all(
                state,
                channel,
                events_tx,
                HostMessage::GameEnded {
                    reason: reason.clone(),
                    snapshot: snap.clone(),
                },
                SessionEvent::GameEnded {
                    reason,
                    snapshot: snap,
                },
            );
            state.ended = true;
            return;
        }
    }

    let lobby = state.lobby_state();
    publish_all(
        state,
        channel,
        events_tx,
        HostMessage::LobbyUpdate { lobby: lobby.clone() },
        SessionEvent::Lobby(lobby),
    );
}

fn handle_intent(
    state: &mut HostState,
    from: MatchboxPeerId,
    msg: PeerMessage,
    events_tx: &UnboundedSender<SessionEvent>,
    channel: &mut WebRtcChannel,
) {
    match msg {
        PeerMessage::Hello {
            nickname,
            proto_version,
        } => {
            if proto_version != PROTO_VERSION {
                send_welcome(channel, state, &from, Some(RejectCode::IncompatibleVersion));
                return;
            }
            if state.game.is_some() {
                send_welcome(channel, state, &from, Some(RejectCode::GameInProgress));
                return;
            }
            if state.peers.len() >= 4 {
                send_welcome(channel, state, &from, Some(RejectCode::LobbyFull));
                return;
            }
            state.peers.insert(from, PeerInfo { nickname });
            send_welcome(channel, state, &from, None);
            let lobby = state.lobby_state();
            publish_all(
                state,
                channel,
                events_tx,
                HostMessage::LobbyUpdate { lobby: lobby.clone() },
                SessionEvent::Lobby(lobby),
            );
        }
        PeerMessage::PickColor { color } => {
            if state.game.is_some() {
                send_error(channel, &from, RejectCode::GameInProgress, "game already started");
                return;
            }
            for slot in state.seats.values_mut() {
                if let Some(seat) = slot {
                    if seat.mb_id == from {
                        *slot = None;
                    }
                }
            }
            if let Some(c) = color {
                if state.seats[c].is_some() {
                    send_error(channel, &from, RejectCode::ColorTaken, "color already taken");
                    return;
                }
                let nickname = state
                    .peers
                    .get(&from)
                    .map(|p| p.nickname.clone())
                    .unwrap_or_else(|| "Player".into());
                state.seats[c] = Some(SeatBinding { mb_id: from, nickname });
            }
            let lobby = state.lobby_state();
            publish_all(
                state,
                channel,
                events_tx,
                HostMessage::LobbyUpdate { lobby: lobby.clone() },
                SessionEvent::Lobby(lobby),
            );
        }
        PeerMessage::StartGame => {
            if from != state.my_mb_id {
                send_error(channel, &from, RejectCode::InvalidMove, "only the host can start");
                return;
            }
            if state.game.is_some() {
                return;
            }
            let seated_count = state.seats.values().filter(|s| s.is_some()).count();
            if seated_count < 2 {
                send_error(
                    channel,
                    &from,
                    RejectCode::InvalidMove,
                    "need at least two seated players",
                );
                return;
            }
            let player_for = |c: Color| -> Player {
                if state.seats[c].is_some() {
                    Player::Someone
                } else {
                    Player::None
                }
            };
            let game = Game::new(
                player_for(Color::Red),
                player_for(Color::Blue),
                player_for(Color::Yellow),
                player_for(Color::Green),
            );
            state.game = Some(game);
            let snap = state.snapshot().unwrap();
            publish_all(
                state,
                channel,
                events_tx,
                HostMessage::GameStarted { snapshot: snap.clone() },
                SessionEvent::GameStarted { snapshot: snap },
            );
        }
        PeerMessage::PlayMove { m } => {
            if !ensure_turn(state, &from, channel) {
                return;
            }
            let game = state.game.as_mut().unwrap();
            if let Err(e) = game.play_move(m) {
                send_error(channel, &from, RejectCode::InvalidMove, &e);
                return;
            }
            let snap = state.snapshot().unwrap();
            publish_all(
                state,
                channel,
                events_tx,
                HostMessage::StateUpdate {
                    snapshot: snap.clone(),
                    last_move: Some(m),
                },
                SessionEvent::StateUpdate {
                    snapshot: snap,
                    last_move: Some(m),
                },
            );
            maybe_end_game(state, channel, events_tx);
        }
        PeerMessage::SkipTurn => {
            if !ensure_turn(state, &from, channel) {
                return;
            }
            let game = state.game.as_mut().unwrap();
            game.skip_turn();
            let snap = state.snapshot().unwrap();
            publish_all(
                state,
                channel,
                events_tx,
                HostMessage::StateUpdate {
                    snapshot: snap.clone(),
                    last_move: None,
                },
                SessionEvent::StateUpdate {
                    snapshot: snap,
                    last_move: None,
                },
            );
            maybe_end_game(state, channel, events_tx);
        }
        PeerMessage::RequestSnapshot => {
            if let Some(snap) = state.snapshot() {
                send_to(
                    channel,
                    &from,
                    &HostMessage::StateUpdate {
                        snapshot: snap,
                        last_move: None,
                    },
                );
            } else {
                send_to(
                    channel,
                    &from,
                    &HostMessage::LobbyUpdate {
                        lobby: state.lobby_state(),
                    },
                );
            }
        }
    }
}

fn ensure_turn(state: &HostState, from: &MatchboxPeerId, channel: &mut WebRtcChannel) -> bool {
    let Some(game) = state.game.as_ref() else {
        send_error(channel, from, RejectCode::InvalidMove, "game not started");
        return false;
    };
    let current = game.currently_playing();
    let Some(seat) = state.seats[current].as_ref() else {
        send_error(channel, from, RejectCode::InvalidMove, "seat is empty");
        return false;
    };
    if &seat.mb_id != from {
        send_error(channel, from, RejectCode::NotYourTurn, "not your turn");
        return false;
    }
    true
}

fn maybe_end_game(
    state: &mut HostState,
    channel: &mut WebRtcChannel,
    events_tx: &UnboundedSender<SessionEvent>,
) {
    let Some(game) = state.game.as_ref() else {
        return;
    };
    if !game.is_game_over() {
        return;
    }
    let scores = game.scores();
    let active: Vec<Color> = state
        .seats
        .iter()
        .filter_map(|(c, s)| s.as_ref().map(|_| c))
        .collect();
    let top = active.iter().map(|&c| scores[c].score).max().unwrap_or(0);
    let winners: Vec<Color> = active
        .iter()
        .copied()
        .filter(|c| scores[*c].score == top)
        .collect();
    let snap = state.snapshot().unwrap();
    let reason = EndReason::Completed { winners };
    publish_all(
        state,
        channel,
        events_tx,
        HostMessage::GameEnded {
            reason: reason.clone(),
            snapshot: snap.clone(),
        },
        SessionEvent::GameEnded {
            reason,
            snapshot: snap,
        },
    );
    state.ended = true;
}

/// Broadcast a host message to every remote peer, and emit the corresponding
/// local event so the host's own UI picks up the change without a round-trip
/// through the data channel.
fn publish_all(
    state: &HostState,
    channel: &mut WebRtcChannel,
    events_tx: &UnboundedSender<SessionEvent>,
    wire: HostMessage,
    local: SessionEvent,
) {
    let bytes = encode(&wire);
    for peer in state.peers.keys().copied().filter(|p| *p != state.my_mb_id) {
        let _ = channel.try_send(bytes.clone().into_boxed_slice(), peer);
    }
    let _ = events_tx.unbounded_send(local);
}

fn send_to(channel: &mut WebRtcChannel, peer: &MatchboxPeerId, msg: &HostMessage) {
    let bytes = encode(msg);
    let _ = channel.try_send(bytes.into_boxed_slice(), *peer);
}

fn send_welcome(
    channel: &mut WebRtcChannel,
    state: &HostState,
    peer: &MatchboxPeerId,
    rejected: Option<RejectCode>,
) {
    if peer == &state.my_mb_id {
        return;
    }
    send_to(
        channel,
        peer,
        &HostMessage::Welcome {
            your_peer_id: mb_to_wire(peer),
            host_peer_id: mb_to_wire(&state.my_mb_id),
            lobby: state.lobby_state(),
            rejected,
        },
    );
}

fn send_error(
    channel: &mut WebRtcChannel,
    peer: &MatchboxPeerId,
    code: RejectCode,
    message: &str,
) {
    send_to(
        channel,
        peer,
        &HostMessage::Error {
            code,
            message: message.into(),
        },
    );
}
