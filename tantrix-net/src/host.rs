//! Host-side session: owns the authoritative `Game` and the lobby.
//!
//! Peers are tracked by a stable [`SessionToken`] (a client-generated UUID
//! persisted in `localStorage`), *not* by the ephemeral matchbox `PeerId`.
//! This lets a peer reload the page, get a fresh matchbox id, and have the
//! host rebind them to their seat — or, if the game is already running and
//! they're new, drop them in as a spectator.

use std::collections::{HashMap, HashSet};
use std::future::Future;

use enum_map::EnumMap;
use futures::future::FutureExt;
use futures::{select, StreamExt};
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use log::{debug, info, warn};
use matchbox_socket::{PeerId as MatchboxPeerId, PeerState, WebRtcChannel};
use tantrix_game::{Color, Game, Hand, Player};
use tantrix_proto::{
    decode, encode, EndReason, GameSnapshot, HostMessage, LobbyState, Participant, ParticipantRole,
    PeerId, PeerMessage, RejectCode, Seat, SessionToken, PROTO_VERSION,
};
use uuid::Uuid;

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

/// Authoritative entry per session token. Survives disconnect; only purged
/// when the game ends or the host tears down.
#[derive(Clone, Debug)]
struct ParticipantRecord {
    nickname: String,
    role: ParticipantRole,
    /// Live matchbox id for this participant, or `None` if currently
    /// disconnected. Seat (token-keyed) is preserved regardless.
    current_peer: Option<MatchboxPeerId>,
    kicked: bool,
}

struct HostState {
    my_mb_id: MatchboxPeerId,
    my_token: SessionToken,
    /// Authoritative roster keyed by stable token.
    participants: HashMap<SessionToken, ParticipantRecord>,
    /// Live-peer reverse lookup. An entry is removed when the peer
    /// disconnects — the `ParticipantRecord` itself stays put.
    peer_to_token: HashMap<MatchboxPeerId, SessionToken>,
    /// Matchbox ids we've kicked — drop any subsequent packets from them.
    kicked_peers: HashSet<MatchboxPeerId>,
    game: Option<Game>,
    seats: EnumMap<Color, Option<SessionToken>>,
    /// `true` after `StartGame`. New joiners from that point become
    /// spectators; seat changes are rejected.
    locked_for_game: bool,
    ended: bool,
}

impl HostState {
    fn new(my_mb_id: MatchboxPeerId, nickname: String) -> Self {
        let my_token = SessionToken(Uuid::new_v4().to_string());
        debug!("host: my token = {}", my_token.as_str());
        let mut participants = HashMap::new();
        participants.insert(
            my_token.clone(),
            ParticipantRecord {
                nickname,
                role: ParticipantRole::Host,
                current_peer: Some(my_mb_id),
                kicked: false,
            },
        );
        let mut peer_to_token = HashMap::new();
        peer_to_token.insert(my_mb_id, my_token.clone());
        Self {
            my_mb_id,
            my_token,
            participants,
            peer_to_token,
            kicked_peers: HashSet::new(),
            game: None,
            seats: EnumMap::default(),
            locked_for_game: false,
            ended: false,
        }
    }

    /// The matchbox id that currently owns `token`, if any.
    fn peer_for_token(&self, token: &SessionToken) -> Option<MatchboxPeerId> {
        self.participants.get(token).and_then(|r| r.current_peer)
    }

    fn token_for_peer(&self, peer: &MatchboxPeerId) -> Option<&SessionToken> {
        self.peer_to_token.get(peer)
    }

    fn wire_seats(&self) -> EnumMap<Color, Option<Seat>> {
        EnumMap::from_fn(|c| {
            self.seats[c].as_ref().and_then(|tok| {
                self.participants.get(tok).map(|r| Seat {
                    peer_id: token_to_wire(tok),
                    nickname: r.nickname.clone(),
                })
            })
        })
    }

    /// Build the on-the-wire `Participant` list — everyone currently in the
    /// roster except kicked entries. `seated_color` is resolved from the
    /// authoritative `seats` map so the UI dot always agrees with whoever
    /// actually holds the seat, regardless of the participant's `role`.
    fn wire_participants(&self) -> Vec<Participant> {
        let mut participants: Vec<Participant> = self
            .participants
            .iter()
            .filter(|(_, r)| !r.kicked)
            .map(|(tok, r)| Participant {
                peer_id: token_to_wire(tok),
                token: tok.clone(),
                nickname: r.nickname.clone(),
                role: r.role.clone(),
                seated_color: self.seated_color_for_token(tok),
                connected: r.current_peer.is_some(),
            })
            .collect();
        participants.sort_by(|a, b| {
            let a_rank = participant_sort_rank(a);
            let b_rank = participant_sort_rank(b);
            a_rank
                .cmp(&b_rank)
                .then_with(|| a.nickname.cmp(&b.nickname))
                .then_with(|| a.token.as_str().cmp(b.token.as_str()))
        });
        participants
    }

    fn lobby_state(&self) -> LobbyState {
        let participants = self.wire_participants();
        let unseated: Vec<Seat> = participants
            .iter()
            .filter(|p| matches!(p.role, ParticipantRole::Unseated))
            .map(|p| Seat {
                peer_id: p.peer_id.clone(),
                nickname: p.nickname.clone(),
            })
            .collect();
        LobbyState {
            host: token_to_wire(&self.my_token),
            seats: self.wire_seats(),
            unseated,
            participants,
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
            participants: self.wire_participants(),
        })
    }

    fn seated_color_for_token(&self, token: &SessionToken) -> Option<Color> {
        for (color, slot) in self.seats.iter() {
            if slot.as_ref() == Some(token) {
                return Some(color);
            }
        }
        None
    }

    fn seated_count(&self) -> usize {
        self.seats.values().filter(|s| s.is_some()).count()
    }

    fn connected_seated_count(&self) -> usize {
        self.seats
            .values()
            .filter_map(|slot| slot.as_ref())
            .filter(|token| {
                self.participants
                    .get(*token)
                    .and_then(|record| record.current_peer)
                    .is_some()
            })
            .count()
    }

    fn all_seated_connected(&self) -> bool {
        self.seated_count() == self.connected_seated_count()
    }

    fn find_token_by_peer_id_str(&self, peer_id: &PeerId) -> Option<SessionToken> {
        // `peer_id` on the wire carries the token string.
        let tok = SessionToken::new(peer_id.as_str());
        if self.participants.contains_key(&tok) {
            Some(tok)
        } else {
            None
        }
    }
}

/// Wire `PeerId` used for an in-room member is the stringified session
/// token, so UI references ("kick this peer") survive a reconnect.
fn token_to_wire(token: &SessionToken) -> PeerId {
    PeerId(token.as_str().to_string())
}

fn participant_sort_rank(p: &Participant) -> u8 {
    match &p.role {
        ParticipantRole::Host => 0,
        ParticipantRole::Seated(_) => 1,
        ParticipantRole::Unseated => 2,
        ParticipantRole::Spectator => 3,
    }
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

    info!("host: room open, waiting for guests (peer id {my_mb_id})");
    let mut state = HostState::new(my_mb_id, nickname);
    let my_wire_id = token_to_wire(&state.my_token);

    let _ = events_tx.unbounded_send(SessionEvent::Connected {
        role: SessionRole::Host,
        my_peer_id: my_wire_id.clone(),
        host_peer_id: my_wire_id,
    });
    let _ = events_tx.unbounded_send(SessionEvent::Lobby(state.lobby_state()));

    // Keep the socket alive after GameEnded so the queued final snapshot can
    // reach clients. A channel send only queues bytes; closing here can drop it.
    loop {
        select! {
            cmd = commands_rx.next() => match cmd {
                Some(SessionCommand::Leave) | None => break,
                Some(other) if !state.ended => {
                    let local = intent_from_command(other);
                    let me = state.my_mb_id;
                    handle_intent(&mut state, me, local, &events_tx, &mut channel);
                }
                Some(_) => {}
            },
            packet = channel.next().fuse() => match packet {
                Some((peer, bytes)) if !state.ended => {
                    if state.kicked_peers.contains(&peer) {
                        continue;
                    }
                    let msg: PeerMessage = match decode(&bytes) {
                        Ok(m) => m,
                        Err(e) => {
                            warn!("host: bad peer message from {peer}: {e}");
                            continue;
                        }
                    };
                    handle_intent(&mut state, peer, msg, &events_tx, &mut channel);
                }
                Some(_) => {}
                None => break,
            },
            peer_update = socket.next().fuse() => match peer_update {
                Some((peer, PeerState::Connected)) => info!("host: WebRTC link to {peer} open"),
                Some((peer, PeerState::Disconnected)) => {
                    info!("host: WebRTC link to {peer} closed");
                    if !state.ended {
                        handle_peer_disconnect(&mut state, &peer, &events_tx, &mut channel);
                    }
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
        SessionCommand::UpdateNickname(n) => PeerMessage::UpdateNickname { nickname: n },
        SessionCommand::Kick(t) => PeerMessage::Kick { target: t },
        SessionCommand::Leave => PeerMessage::RequestSnapshot, // never actually dispatched
    }
}

fn handle_peer_disconnect(
    state: &mut HostState,
    peer: &MatchboxPeerId,
    events_tx: &UnboundedSender<SessionEvent>,
    channel: &mut WebRtcChannel,
) {
    // Translate peer → token, then mark the record disconnected but keep it.
    let Some(token) = state.peer_to_token.remove(peer) else {
        return;
    };
    state.kicked_peers.remove(peer);
    if let Some(record) = state.participants.get_mut(&token) {
        record.current_peer = None;
    }

    // Broadcast updated roster so everyone sees the connection dot flip.
    broadcast_roster_change(state, channel, events_tx);
}

/// After a roster-affecting change (join, leave, kick, nickname update),
/// push a `LobbyUpdate` if we're still in the lobby or a `StateUpdate`
/// with `last_move: None` if a game is running.
fn broadcast_roster_change(
    state: &HostState,
    channel: &mut WebRtcChannel,
    events_tx: &UnboundedSender<SessionEvent>,
) {
    if state.game.is_some() {
        let Some(snap) = state.snapshot() else {
            return;
        };
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
    } else {
        let lobby = state.lobby_state();
        publish_all(
            state,
            channel,
            events_tx,
            HostMessage::LobbyUpdate {
                lobby: lobby.clone(),
            },
            SessionEvent::Lobby(lobby),
        );
    }
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
            session_token,
        } => {
            handle_hello(
                state,
                from,
                nickname,
                proto_version,
                session_token,
                events_tx,
                channel,
            );
        }
        PeerMessage::PickColor { color } => {
            if state.locked_for_game {
                reject(
                    channel,
                    events_tx,
                    state.my_mb_id,
                    &from,
                    RejectCode::GameInProgress,
                    "game already started",
                );
                return;
            }
            let Some(token) = state.token_for_peer(&from).cloned() else {
                return;
            };
            // Validate before mutating anything: a rejected pick must leave the
            // sender's current seat exactly as it was.
            if let Some(c) = color {
                if state.seats[c]
                    .as_ref()
                    .is_some_and(|holder| *holder != token)
                {
                    reject(
                        channel,
                        events_tx,
                        state.my_mb_id,
                        &from,
                        RejectCode::ColorTaken,
                        "color already taken",
                    );
                    return;
                }
            }
            // Release any seat they currently hold.
            for slot in state.seats.values_mut() {
                if slot.as_ref() == Some(&token) {
                    *slot = None;
                }
            }
            // Reset role to Unseated; re-seat below if a color is chosen.
            if let Some(r) = state.participants.get_mut(&token) {
                if !matches!(r.role, ParticipantRole::Host) {
                    r.role = ParticipantRole::Unseated;
                }
            }
            if let Some(c) = color {
                state.seats[c] = Some(token.clone());
                if let Some(r) = state.participants.get_mut(&token) {
                    if !matches!(r.role, ParticipantRole::Host) {
                        r.role = ParticipantRole::Seated(c);
                    }
                }
            }
            broadcast_roster_change(state, channel, events_tx);
        }
        PeerMessage::StartGame => {
            if from != state.my_mb_id {
                reject(
                    channel,
                    events_tx,
                    state.my_mb_id,
                    &from,
                    RejectCode::InvalidMove,
                    "only the host can start",
                );
                return;
            }
            if state.game.is_some() {
                return;
            }
            let seated_count = state.seated_count();
            if seated_count < 2 {
                reject(
                    channel,
                    events_tx,
                    state.my_mb_id,
                    &from,
                    RejectCode::InvalidMove,
                    "need at least two seated players",
                );
                return;
            }
            if !state.all_seated_connected() {
                reject(
                    channel,
                    events_tx,
                    state.my_mb_id,
                    &from,
                    RejectCode::InvalidMove,
                    "all seated players must be connected",
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
            )
            .expect("host validates at least two seated players before starting");
            state.game = Some(game);
            state.locked_for_game = true;
            // Everyone still listed as Unseated becomes a Spectator.
            for record in state.participants.values_mut() {
                if matches!(record.role, ParticipantRole::Unseated) {
                    record.role = ParticipantRole::Spectator;
                }
            }
            let snap = state.snapshot().unwrap();
            publish_all(
                state,
                channel,
                events_tx,
                HostMessage::GameStarted {
                    snapshot: snap.clone(),
                },
                SessionEvent::GameStarted { snapshot: snap },
            );
            auto_skip_stuck_players(state, channel, events_tx);
        }
        PeerMessage::PlayMove { m } => {
            if !ensure_turn(state, &from, channel, events_tx) {
                return;
            }
            let game = state.game.as_mut().unwrap();
            if let Err(e) = game.play_move(m) {
                reject(
                    channel,
                    events_tx,
                    state.my_mb_id,
                    &from,
                    RejectCode::InvalidMove,
                    &e.to_string(),
                );
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
            if !maybe_end_game(state, channel, events_tx) {
                auto_skip_stuck_players(state, channel, events_tx);
            }
        }
        PeerMessage::SkipTurn => {
            if !ensure_turn(state, &from, channel, events_tx) {
                return;
            }
            let game = state.game.as_mut().unwrap();
            if !game.get_possible_moves().is_empty() {
                reject(
                    channel,
                    events_tx,
                    state.my_mb_id,
                    &from,
                    RejectCode::InvalidMove,
                    "cannot skip while legal moves exist",
                );
                return;
            }
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
            if !maybe_end_game(state, channel, events_tx) {
                auto_skip_stuck_players(state, channel, events_tx);
            }
        }
        PeerMessage::UpdateNickname { nickname } => {
            let trimmed = nickname.trim();
            if trimmed.is_empty() {
                reject(
                    channel,
                    events_tx,
                    state.my_mb_id,
                    &from,
                    RejectCode::InvalidMove,
                    "nickname cannot be empty",
                );
                return;
            }
            let new_nick = trimmed.to_string();
            let Some(token) = state.token_for_peer(&from).cloned() else {
                return;
            };
            if let Some(r) = state.participants.get_mut(&token) {
                r.nickname = new_nick;
            }
            broadcast_roster_change(state, channel, events_tx);
        }
        PeerMessage::Kick { target } => {
            handle_kick(state, from, target, events_tx, channel);
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

fn handle_hello(
    state: &mut HostState,
    from: MatchboxPeerId,
    nickname: String,
    proto_version: u32,
    session_token: SessionToken,
    events_tx: &UnboundedSender<SessionEvent>,
    channel: &mut WebRtcChannel,
) {
    if proto_version != PROTO_VERSION {
        send_welcome(channel, state, &from, Some(RejectCode::IncompatibleVersion));
        return;
    }
    debug!(
        "host: Hello from mb={} token={} nick={}",
        from,
        session_token.as_str(),
        nickname
    );

    // Existing participant under this token?
    if let Some(record) = state.participants.get(&session_token).cloned() {
        if record.kicked {
            send_welcome(channel, state, &from, Some(RejectCode::Kicked));
            return;
        }
        // Rebind: same token, new matchbox peer.
        // If a previous peer still held this token, forget the old mapping.
        if let Some(old_peer) = record.current_peer {
            state.peer_to_token.remove(&old_peer);
        }
        {
            let r = state.participants.get_mut(&session_token).unwrap();
            r.current_peer = Some(from);
            // Keep the nickname the host already has on file. Reloads
            // regenerate a random Hello nickname; rebinding to that would
            // rename the returning player every reconnect. The player can
            // still change it via `UpdateNickname` after rejoining.
            let _ = nickname;
        }
        state.peer_to_token.insert(from, session_token.clone());

        send_welcome(channel, state, &from, None);
        // Replay current game state so the returning client can resume.
        if let Some(snap) = state.snapshot() {
            send_to(channel, &from, &HostMessage::GameStarted { snapshot: snap });
        }
        broadcast_roster_change(state, channel, events_tx);
        return;
    }

    // New participant.
    let trimmed = nickname.trim();
    let nick = if trimmed.is_empty() {
        "Player".to_string()
    } else {
        trimmed.to_string()
    };
    let role = if state.locked_for_game {
        ParticipantRole::Spectator
    } else {
        ParticipantRole::Unseated
    };
    state.participants.insert(
        session_token.clone(),
        ParticipantRecord {
            nickname: nick,
            role,
            current_peer: Some(from),
            kicked: false,
        },
    );
    state.peer_to_token.insert(from, session_token);

    send_welcome(channel, state, &from, None);
    if let Some(snap) = state.snapshot() {
        send_to(channel, &from, &HostMessage::GameStarted { snapshot: snap });
    }
    broadcast_roster_change(state, channel, events_tx);
}

fn handle_kick(
    state: &mut HostState,
    from: MatchboxPeerId,
    target: PeerId,
    events_tx: &UnboundedSender<SessionEvent>,
    channel: &mut WebRtcChannel,
) {
    if from != state.my_mb_id {
        reject(
            channel,
            events_tx,
            state.my_mb_id,
            &from,
            RejectCode::InvalidMove,
            "only the host can kick",
        );
        return;
    }
    let Some(token) = state.find_token_by_peer_id_str(&target) else {
        return;
    };
    if token == state.my_token {
        // Can't kick self.
        return;
    }
    // Capture the target's live peer (if any) before we clear the record.
    let target_peer = state.peer_for_token(&token);

    // If they were seated in a running game, retire their color cleanly.
    let seated_color = state.seated_color_for_token(&token);
    if let Some(color) = seated_color {
        state.seats[color] = None;
        if let Some(game) = state.game.as_mut() {
            let _ = game.retire_color(color);
        }
    }

    if let Some(record) = state.participants.get_mut(&token) {
        record.kicked = true;
        record.role = ParticipantRole::Spectator;
        record.current_peer = None;
    }
    if let Some(peer) = target_peer {
        state.peer_to_token.remove(&peer);
        state.kicked_peers.insert(peer);
    }

    // Tell everyone — including the kicked peer — so the UI flips.
    let wire = HostMessage::Kicked {
        target: target.clone(),
        reason: "Removed by host".into(),
    };
    let bytes = encode(&wire);
    for peer in state.peer_to_token.keys().copied() {
        if peer != state.my_mb_id {
            let _ = channel.try_send(bytes.clone().into_boxed_slice(), peer);
        }
    }
    if let Some(peer) = target_peer {
        let _ = channel.try_send(bytes.clone().into_boxed_slice(), peer);
    }

    // After a kick, drop the participant record entirely so the roster
    // reflects reality (we already notified them directly).
    state.participants.remove(&token);

    // Retiring a seat can end the game (one player left) or just reduce
    // the turn cycle. Check before broadcasting so the roster update
    // shipped to clients already reflects the new playing_colors.
    if maybe_end_game(state, channel, events_tx) {
        return;
    }
    broadcast_roster_change(state, channel, events_tx);
}

fn ensure_turn(
    state: &HostState,
    from: &MatchboxPeerId,
    channel: &mut WebRtcChannel,
    events_tx: &UnboundedSender<SessionEvent>,
) -> bool {
    let Some(game) = state.game.as_ref() else {
        reject(
            channel,
            events_tx,
            state.my_mb_id,
            from,
            RejectCode::InvalidMove,
            "game not started",
        );
        return false;
    };
    let current = game.currently_playing();
    let Some(seat_token) = state.seats[current].as_ref() else {
        reject(
            channel,
            events_tx,
            state.my_mb_id,
            from,
            RejectCode::InvalidMove,
            "seat is empty",
        );
        return false;
    };
    let from_token = state.token_for_peer(from);
    if from_token != Some(seat_token) {
        reject(
            channel,
            events_tx,
            state.my_mb_id,
            from,
            RejectCode::NotYourTurn,
            "not your turn",
        );
        return false;
    }
    true
}

fn auto_skip_stuck_players(
    state: &mut HostState,
    channel: &mut WebRtcChannel,
    events_tx: &UnboundedSender<SessionEvent>,
) {
    let Some(max_skips) = state.game.as_ref().map(|game| game.playing_colors().len()) else {
        return;
    };
    let mut skips = 0;
    for _ in 0..max_skips {
        let should_skip = {
            let Some(game) = state.game.as_mut() else {
                return;
            };
            if game.is_game_over() || !game.get_possible_moves().is_empty() {
                false
            } else {
                game.skip_turn();
                true
            }
        };
        if !should_skip {
            break;
        }
        skips += 1;
        let Some(snap) = state.snapshot() else {
            return;
        };
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
    }
    if skips == max_skips {
        let no_player_can_move = state
            .game
            .as_mut()
            .map(|game| !game.is_game_over() && game.get_possible_moves().is_empty())
            .unwrap_or(false);
        if no_player_can_move {
            finish_game(state, channel, events_tx);
            return;
        }
    }
    maybe_end_game(state, channel, events_tx);
}

/// If the game is over (by completion or because only one player remains
/// after retirement), broadcast the ending and return `true`.
fn maybe_end_game(
    state: &mut HostState,
    channel: &mut WebRtcChannel,
    events_tx: &UnboundedSender<SessionEvent>,
) -> bool {
    let Some(game) = state.game.as_ref() else {
        return false;
    };
    let playing = game.playing_colors();
    let natural_end = game.is_game_over();
    let only_one_left = playing.len() <= 1;

    if !natural_end && !only_one_left {
        return false;
    }

    finish_game(state, channel, events_tx);
    true
}

fn finish_game(
    state: &mut HostState,
    channel: &mut WebRtcChannel,
    events_tx: &UnboundedSender<SessionEvent>,
) {
    let Some(game) = state.game.as_ref() else {
        return;
    };
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
    for peer in state.peer_to_token.keys().copied() {
        if peer != state.my_mb_id {
            let _ = channel.try_send(bytes.clone().into_boxed_slice(), peer);
        }
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
    // `your_peer_id` uses the token-as-PeerId mapping so the client
    // stores a stable identifier that matches what the host broadcasts.
    let your_id = state
        .token_for_peer(peer)
        .map(token_to_wire)
        .unwrap_or_else(|| PeerId(peer.to_string()));
    send_to(
        channel,
        peer,
        &HostMessage::Welcome {
            your_peer_id: your_id,
            host_peer_id: token_to_wire(&state.my_token),
            lobby: state.lobby_state(),
            rejected,
        },
    );
}

/// Reject an intent from `peer`.
///
/// The host also feeds its own UI commands through `handle_intent`, so `peer`
/// can be the host itself. There is no data channel to oneself: matchbox
/// panics (`couldn't find data channel for peer`) on a send to an unknown
/// peer, which would take the whole session down. Errors for the host go
/// straight to its own event stream instead.
fn reject(
    channel: &mut WebRtcChannel,
    events_tx: &UnboundedSender<SessionEvent>,
    me: MatchboxPeerId,
    peer: &MatchboxPeerId,
    code: RejectCode,
    message: &str,
) {
    if *peer == me {
        let _ = events_tx.unbounded_send(SessionEvent::Error {
            code: Some(code),
            message: message.into(),
        });
        return;
    }
    send_to(
        channel,
        peer,
        &HostMessage::Error {
            code,
            message: message.into(),
        },
    );
}
