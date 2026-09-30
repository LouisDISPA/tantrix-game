//! Pure room rules. Each server adapter owns persistence and sockets; this
//! module owns the roster and applies game intents without trusting clients.

use std::collections::HashSet;

use enum_map::EnumMap;
use serde::{Deserialize, Serialize};
use tantrix_game::{Color, Game, GameData, Hand, Move, Player};
use tantrix_proto::{
    EndReason, GameSnapshot, HostMessage, LobbyState, Participant, ParticipantRole, PeerId,
    PeerMessage, RejectCode, Seat,
};

use crate::session_cookie::SessionToken;

/// Only four colors can play, but anyone may wait in the lobby for a seat or
/// watch the game. This bound only keeps one room's storage and broadcasts
/// small if someone scripts joins.
pub const MAX_MEMBERS: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerRecord {
    pub id: PeerId,
    pub token: SessionToken,
    pub nickname: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoomData {
    pub players: Vec<PlayerRecord>,
    pub seats: EnumMap<Color, Option<SessionToken>>,
    pub game: Option<GameData>,
    pub ended: bool,
    /// Retains an early end across reloads; older saved rooms had only `ended`.
    #[serde(default)]
    pub end_reason: Option<EndReason>,
}

pub struct Change {
    pub message: HostMessage,
    pub persist: bool,
}

impl RoomData {
    pub fn player(&self, token: &SessionToken) -> Option<&PlayerRecord> {
        self.players.iter().find(|p| &p.token == token)
    }

    pub fn color_of(&self, token: &SessionToken) -> Option<Color> {
        self.seats
            .iter()
            .find_map(|(c, seat)| (seat.as_ref() == Some(token)).then_some(c))
    }

    fn wire_seats(&self) -> EnumMap<Color, Option<Seat>> {
        EnumMap::from_fn(|c| {
            self.seats[c]
                .as_ref()
                .and_then(|token| self.player(token))
                .map(|p| Seat {
                    peer_id: p.id.clone(),
                    nickname: p.nickname.clone(),
                })
        })
    }

    fn participants(&self, connected: &HashSet<SessionToken>) -> Vec<Participant> {
        self.players
            .iter()
            .map(|p| {
                let color = self.color_of(&p.token);
                Participant {
                    peer_id: p.id.clone(),
                    nickname: p.nickname.clone(),
                    role: color.map_or(ParticipantRole::Unseated, ParticipantRole::Seated),
                    seated_color: color,
                    connected: connected.contains(&p.token),
                }
            })
            .collect()
    }

    pub fn lobby(&self, connected: &HashSet<SessionToken>) -> LobbyState {
        let participants = self.participants(connected);
        let unseated = participants
            .iter()
            .filter(|p| p.seated_color.is_none())
            .map(|p| Seat {
                peer_id: p.peer_id.clone(),
                nickname: p.nickname.clone(),
            })
            .collect();
        LobbyState {
            seats: self.wire_seats(),
            unseated,
            participants,
        }
    }

    pub fn snapshot(&self, connected: &HashSet<SessionToken>) -> Option<GameSnapshot> {
        let game = Game::from_data(self.game.clone()?);
        let playing_colors = game.playing_colors().to_vec();
        let mut hands: EnumMap<Color, Hand> = EnumMap::default();
        for &color in &playing_colors {
            hands[color] = game.hand(color).clone();
        }
        Some(GameSnapshot {
            board: game.board().clone(),
            hands,
            bag_count: game.bag_count(),
            current_player: game.current_player(),
            turn_step: game.turn_step(),
            playing_colors,
            seats: self.wire_seats(),
            participants: self.participants(connected),
        })
    }

    /// Register a new member or resume an existing one. Newcomers are welcome
    /// at any time: during or after a game they join without a seat and
    /// watch, and can take over a seat with `ReplaceSeat`.
    pub fn hello(
        &mut self,
        token: SessionToken,
        nickname: String,
        new_id: PeerId,
    ) -> Result<PeerId, RejectCode> {
        if let Some(existing) = self.player(&token) {
            return Ok(existing.id.clone());
        }
        if self.players.len() >= MAX_MEMBERS {
            return Err(RejectCode::RoomFull);
        }
        let nickname = nickname.trim();
        if nickname.is_empty() || nickname.chars().count() > 24 {
            return Err(RejectCode::InvalidMove);
        }
        self.players.push(PlayerRecord {
            id: new_id.clone(),
            token,
            nickname: nickname.to_string(),
        });
        Ok(new_id)
    }

    /// A seated player keeps their seat when their socket drops, in the
    /// lobby and in a game, so a reload with the same cookie resumes it;
    /// anyone may take it over meanwhile (`ReplaceSeat`). Members without a
    /// seat have nothing to keep and are removed.
    pub fn disconnect(&mut self, token: &SessionToken) -> bool {
        if self.color_of(token).is_some() {
            return false;
        }
        let before = self.players.len();
        self.players.retain(|p| &p.token != token);
        self.players.len() != before
    }

    pub fn apply(
        &mut self,
        token: &SessionToken,
        intent: PeerMessage,
        connected: &HashSet<SessionToken>,
    ) -> Result<Change, (RejectCode, String)> {
        if self.player(token).is_none() {
            return Err((RejectCode::InvalidMove, "join the room first".into()));
        }
        match intent {
            PeerMessage::Hello { .. } => Err((RejectCode::InvalidMove, "already joined".into())),
            PeerMessage::PickColor { color } => {
                if self.game.is_some() {
                    return Err((RejectCode::GameInProgress, "game already started".into()));
                }
                if let Some(c) = color {
                    if self.seats[c].as_ref().is_some_and(|owner| owner != token) {
                        return Err((RejectCode::ColorTaken, "color already taken".into()));
                    }
                }
                for seat in self.seats.values_mut() {
                    if seat.as_ref() == Some(token) {
                        *seat = None;
                    }
                }
                if let Some(c) = color {
                    self.seats[c] = Some(token.clone());
                }
                Ok(Change {
                    message: HostMessage::LobbyUpdate {
                        lobby: self.lobby(connected),
                    },
                    persist: true,
                })
            }
            PeerMessage::ReplaceSeat { color } => {
                if self.ended {
                    return Err((RejectCode::InvalidMove, "the game is over".into()));
                }
                let mine = self.color_of(token);
                if mine == Some(color) {
                    return Err((RejectCode::InvalidMove, "that is already your seat".into()));
                }
                if let Some(data) = &self.game {
                    // Mid-game a seat can't be left empty, so only someone
                    // without one can step in.
                    if mine.is_some() {
                        return Err((RejectCode::InvalidMove, "you already have a seat".into()));
                    }
                    if !Game::from_data(data.clone())
                        .playing_colors()
                        .contains(&color)
                    {
                        return Err((RejectCode::InvalidMove, "that color is not playing".into()));
                    }
                } else if let Some(old) = mine {
                    self.seats[old] = None;
                }
                let displaced = self.seats[color].replace(token.clone());
                // The replaced player stays in the room without a seat, or
                // leaves it if they are offline (as any unseated member would).
                if let Some(owner) = displaced.filter(|owner| !connected.contains(owner)) {
                    self.players.retain(|p| p.token != owner);
                }
                Ok(Change {
                    message: self.roster_message(connected),
                    persist: true,
                })
            }
            PeerMessage::StartGame => {
                if self.game.is_some() {
                    return Err((RejectCode::GameInProgress, "game already started".into()));
                }
                if self.color_of(token).is_none() {
                    return Err((RejectCode::InvalidMove, "choose a color first".into()));
                }
                let seated: Vec<_> = self.seats.values().filter_map(Option::as_ref).collect();
                if seated.len() < 2 || !seated.iter().all(|t| connected.contains(*t)) {
                    return Err((
                        RejectCode::InvalidMove,
                        "need at least two connected seated players".into(),
                    ));
                }
                let player_for = |c: Color| {
                    if self.seats[c].is_some() {
                        Player::Someone
                    } else {
                        Player::None
                    }
                };
                let mut game = Game::new(
                    player_for(Color::Red),
                    player_for(Color::Blue),
                    player_for(Color::Yellow),
                    player_for(Color::Green),
                )
                .map_err(|e| (RejectCode::InvalidMove, e.to_string()))?;
                self.ended = game.skip_stuck_players();
                self.game = Some(game.to_data());
                Ok(Change {
                    message: self.current_message(connected),
                    persist: true,
                })
            }
            PeerMessage::PlayMove { placement } => {
                let mut game = self.turn_game(token)?;
                let played = game
                    .play_move(placement)
                    .map_err(|e| (RejectCode::InvalidMove, e.to_string()))?;
                let stalled = game.skip_stuck_players();
                self.finish_turn(game, Some(played.m), stalled, connected)
            }
            PeerMessage::SkipTurn => {
                let mut game = self.turn_game(token)?;
                if !game.legal_moves().is_empty() {
                    return Err((RejectCode::InvalidMove, "legal moves remain".into()));
                }
                game.skip_turn();
                let stalled = game.skip_stuck_players();
                self.finish_turn(game, None, stalled, connected)
            }
            PeerMessage::EndGame => {
                let Some(data) = &self.game else {
                    return Err((RejectCode::InvalidMove, "game has not started".into()));
                };
                if self.ended {
                    return Err((RejectCode::InvalidMove, "the game is over".into()));
                }
                let Some(by) = self.color_of(token) else {
                    return Err((RejectCode::InvalidMove, "choose a color first".into()));
                };
                let game = Game::from_data(data.clone());
                let reason = EndReason::EndedEarly {
                    by,
                    winners: game.winners(),
                };
                self.ended = true;
                self.end_reason = Some(reason.clone());
                Ok(Change {
                    message: HostMessage::GameEnded {
                        reason,
                        snapshot: self
                            .snapshot(connected)
                            .expect("running game has a snapshot"),
                    },
                    persist: true,
                })
            }
            PeerMessage::RequestSnapshot => Ok(Change {
                message: self.current_message(connected),
                persist: false,
            }),
            PeerMessage::UpdateNickname { nickname } => {
                let nickname = nickname.trim();
                if nickname.is_empty() || nickname.chars().count() > 24 {
                    return Err((
                        RejectCode::InvalidMove,
                        "nickname must be 1–24 characters".into(),
                    ));
                }
                if let Some(player) = self.players.iter_mut().find(|p| &p.token == token) {
                    player.nickname = nickname.to_string();
                }
                Ok(Change {
                    message: self.current_message(connected),
                    persist: true,
                })
            }
        }
    }

    fn turn_game(&self, token: &SessionToken) -> Result<Game, (RejectCode, String)> {
        let Some(data) = &self.game else {
            return Err((RejectCode::InvalidMove, "game has not started".into()));
        };
        let game = Game::from_data(data.clone());
        if self.ended || self.seats[game.current_player()].as_ref() != Some(token) {
            return Err((RejectCode::NotYourTurn, "not your turn".into()));
        }
        Ok(game)
    }

    fn finish_turn(
        &mut self,
        game: Game,
        last_move: Option<Move>,
        stalled: bool,
        connected: &HashSet<SessionToken>,
    ) -> Result<Change, (RejectCode, String)> {
        self.ended = game.is_game_over() || stalled;
        if self.ended {
            self.end_reason = Some(EndReason::Completed {
                winners: game.winners(),
            });
        }
        self.game = Some(game.to_data());
        let snapshot = self.snapshot(connected).expect("game just saved");
        let message = if self.ended {
            HostMessage::GameEnded {
                reason: self.end_reason.clone().expect("ended game has a reason"),
                snapshot,
            }
        } else {
            HostMessage::StateUpdate {
                snapshot,
                last_move,
            }
        };
        Ok(Change {
            message,
            persist: true,
        })
    }

    pub fn current_message(&self, connected: &HashSet<SessionToken>) -> HostMessage {
        match self.snapshot(connected) {
            Some(snapshot) if self.ended => {
                let game = Game::from_data(self.game.clone().expect("ended game has data"));
                HostMessage::GameEnded {
                    reason: self
                        .end_reason
                        .clone()
                        .unwrap_or_else(|| EndReason::Completed {
                            winners: game.winners(),
                        }),
                    snapshot,
                }
            }
            Some(snapshot) => HostMessage::GameStarted { snapshot },
            None => HostMessage::LobbyUpdate {
                lobby: self.lobby(connected),
            },
        }
    }

    /// Announce participant connectivity without replaying the game-start
    /// transition to players who are already on the game screen.
    pub fn roster_message(&self, connected: &HashSet<SessionToken>) -> HostMessage {
        if self.game.is_some() && !self.ended {
            HostMessage::StateUpdate {
                snapshot: self.snapshot(connected).expect("running game has data"),
                last_move: None,
            }
        } else {
            self.current_message(connected)
        }
    }
}
