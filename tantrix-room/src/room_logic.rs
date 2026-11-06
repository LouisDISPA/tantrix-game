//! Pure room rules. [`crate::engine`] runs them for each socket event; this
//! module owns the roster and applies game intents without trusting clients.

use std::collections::HashSet;

use enum_map::EnumMap;
use serde::{Deserialize, Serialize};
use tantrix_game::{Color, Game, GameData, GameStatus, Hand, Move, Player};
use tantrix_proto::{
    EndReason, GameSnapshot, HostMessage, LobbyState, Participant, ParticipantRole, PeerId,
    PeerMessage, RejectCode, Seat,
};

use crate::session_cookie::SessionToken;

/// Only four colors can play, but anyone may wait in the lobby for a seat or
/// watch the game. This bound only keeps one room's storage and broadcasts
/// small if someone scripts joins.
pub const MAX_MEMBERS: usize = 32;

/// Longest nickname, in characters.
pub const MAX_NICKNAME_CHARS: usize = 24;

/// `nickname` as the room shows it: trimmed, without control characters,
/// text-direction overrides or the invisible characters that make a name
/// look blank. Joiners and variation selectors stay, as emoji and some
/// scripts need them. `None` if no visible character is left, or the name
/// is longer than [`MAX_NICKNAME_CHARS`].
pub fn clean_nickname(nickname: &str) -> Option<String> {
    let cleaned: String = nickname.chars().filter(|&c| !is_hidden(c)).collect();
    let cleaned = cleaned.trim();
    let visible = cleaned
        .chars()
        .any(|c| !c.is_whitespace() && !is_modifier(c));
    (visible && cleaned.chars().count() <= MAX_NICKNAME_CHARS).then(|| cleaned.to_string())
}

/// Characters that show nothing, or change how the text around them
/// shows, and that no name needs.
fn is_hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}' // soft hyphen
                | '\u{034F}' // combining grapheme joiner
                | '\u{061C}' // Arabic letter mark
                | '\u{115F}' | '\u{1160}' | '\u{3164}' | '\u{FFA0}' // Hangul fillers
                | '\u{17B4}' | '\u{17B5}' // Khmer inherent vowels
                | '\u{200B}' // zero-width space
                | '\u{200E}' | '\u{200F}' // direction marks
                | '\u{2028}'..='\u{202E}' // line breaks, direction embeddings and overrides
                | '\u{2060}'..='\u{206F}' // word joiner, invisible operators, direction isolates
                | '\u{FEFF}' // zero-width no-break space
                | '\u{FFF9}'..='\u{FFFB}' // interlinear annotations
        )
}

/// Characters kept in names that only modify their neighbours: joiners,
/// variation selectors, emoji tags. A name of only these looks blank.
fn is_modifier(c: char) -> bool {
    matches!(
        c,
        '\u{180B}'..='\u{180F}'
            | '\u{200C}'
            | '\u{200D}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{E0000}'..='\u{E007F}'
            | '\u{E0100}'..='\u{E01EF}'
    )
}

/// What shuffles a match's bag and turn order. The hidden bag order is the
/// game's secret, so it is the OS-seeded thread RNG (ChaCha12), not a 64-bit
/// seed, and no seed is kept. Nothing needs one: `GameData` keeps the bag
/// and the full history, and undoing the history gives back the deal.
/// Snapshots carry neither.
#[cfg(not(test))]
#[expect(
    clippy::disallowed_methods,
    reason = "online deals stay secret: no seed exists"
)]
fn deal_rng(_match_id: u32) -> impl rand::Rng {
    rand::rng()
}

/// Unit tests deal match `n` from seed `n`, so they play the same every run.
#[cfg(test)]
fn deal_rng(match_id: u32) -> impl rand::Rng {
    use rand::SeedableRng;
    rand::rngs::Xoshiro256PlusPlus::seed_from_u64(match_id.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerRecord {
    pub id: PeerId,
    pub token: SessionToken,
    pub nickname: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoomData {
    #[serde(default)]
    pub match_id: u32,
    pub players: Vec<PlayerRecord>,
    pub seats: EnumMap<Color, Option<SessionToken>>,
    pub game: Option<GameData>,
    pub ended: bool,
    /// Retains an early end across reloads; older saved rooms had only `ended`.
    #[serde(default)]
    pub end_reason: Option<EndReason>,
}

/// Immutable server-side archive; draw order/history never enter client messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedMatch {
    pub id: u32,
    pub game: GameData,
    pub seats: EnumMap<Color, Option<Seat>>,
    pub reason: EndReason,
}

pub struct Change {
    pub message: HostMessage,
    pub persist: bool,
}

impl RoomData {
    pub fn has_started(&self) -> bool {
        self.game.is_some() || self.match_id > 0
    }

    pub fn completed_match(&self) -> Option<CompletedMatch> {
        if !self.ended {
            return None;
        }
        let game = self.game.clone()?;
        let reason = self
            .end_reason
            .clone()
            .unwrap_or_else(|| EndReason::Completed {
                winners: Game::from_data(game.clone()).winners(),
            });
        Some(CompletedMatch {
            id: self.match_id,
            game,
            seats: self.wire_seats(),
            reason,
        })
    }

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
            match_id: self.match_id,
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
            match_id: self.match_id,
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
    /// watch, and can take over a disconnected player's seat with
    /// `ReplaceSeat`.
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
        let nickname = clean_nickname(&nickname).ok_or(RejectCode::InvalidMove)?;
        self.players.push(PlayerRecord {
            id: new_id.clone(),
            token,
            nickname,
        });
        Ok(new_id)
    }

    /// A seated player keeps their seat when their socket drops, in the
    /// lobby and in a game, so a reload with the same cookie resumes it;
    /// someone else may take it over meanwhile (`ReplaceSeat`). Members
    /// without a seat have nothing to keep and are removed.
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
        // One match number scopes play and restart commands, including clicks
        // still in flight when another player opens the next lobby.
        let match_id = match &intent {
            PeerMessage::StartGame { match_id }
            | PeerMessage::PlayMove { match_id, .. }
            | PeerMessage::EndGame { match_id }
            | PeerMessage::Rematch { match_id } => Some(*match_id),
            _ => None,
        };
        if let Some(match_id) = match_id {
            if match_id != self.match_id {
                if matches!(intent, PeerMessage::Rematch { .. }) && match_id < self.match_id {
                    return Ok(Change {
                        message: self.current_message(connected),
                        persist: false,
                    });
                }
                return Err((
                    RejectCode::InvalidMove,
                    "that match is no longer active".into(),
                ));
            }
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
                    // Taking over keeps a game going when a player drops out;
                    // nobody can take the hand of someone still playing it.
                    if self.seats[color]
                        .as_ref()
                        .is_some_and(|owner| connected.contains(owner))
                    {
                        return Err((
                            RejectCode::ColorTaken,
                            "that player is still connected".into(),
                        ));
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
            PeerMessage::StartGame { .. } => {
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
                let players = EnumMap::from_fn(|c| {
                    if self.seats[c].is_some() {
                        Player::Someone
                    } else {
                        Player::None
                    }
                });
                let mut game = Game::from_players_with_rng(players, &mut deal_rng(self.match_id))
                    .map_err(|e| (RejectCode::InvalidMove, e.to_string()))?;
                game.skip_stuck_players();
                self.ended = game.status() != GameStatus::Playing;
                self.game = Some(game.to_data());
                Ok(Change {
                    message: self.current_message(connected),
                    persist: true,
                })
            }
            PeerMessage::PlayMove { placement, .. } => {
                let mut game = self.turn_game(token)?;
                let played = game
                    .play_move(placement)
                    .map_err(|e| (RejectCode::InvalidMove, e.to_string()))?;
                game.skip_stuck_players();
                self.finish_turn(game, played.m, connected)
            }
            PeerMessage::EndGame { .. } => {
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
            PeerMessage::Rematch { .. } => {
                if !self.ended || self.color_of(token).is_none() {
                    return Err((
                        RejectCode::InvalidMove,
                        "only a seated player can restart a finished match".into(),
                    ));
                }
                self.match_id += 1;
                self.game = None;
                self.ended = false;
                self.end_reason = None;
                for seat in self.seats.values_mut() {
                    if seat
                        .as_ref()
                        .is_some_and(|owner| !connected.contains(owner))
                    {
                        *seat = None;
                    }
                }
                self.players
                    .retain(|player| connected.contains(&player.token));
                Ok(Change {
                    message: HostMessage::LobbyUpdate {
                        lobby: self.lobby(connected),
                    },
                    persist: true,
                })
            }
            PeerMessage::RequestSnapshot => Ok(Change {
                message: self.current_message(connected),
                persist: false,
            }),
            PeerMessage::UpdateNickname { nickname } => {
                let Some(nickname) = clean_nickname(&nickname) else {
                    return Err((
                        RejectCode::InvalidMove,
                        "nickname must be 1–24 characters".into(),
                    ));
                };
                // The same name again changes nothing: no save, no broadcast.
                let persist = match self.players.iter_mut().find(|p| &p.token == token) {
                    Some(player) if player.nickname != nickname => {
                        player.nickname = nickname;
                        true
                    }
                    _ => false,
                };
                Ok(Change {
                    message: self.roster_message(connected),
                    persist,
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
        last_move: Move,
        connected: &HashSet<SessionToken>,
    ) -> Result<Change, (RejectCode, String)> {
        // Finished or stalled: either way nobody can move any more.
        self.ended = game.status() != GameStatus::Playing;
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
                last_move: Some(last_move),
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

#[cfg(test)]
mod tests {
    use tantrix_game::{Board, Position, Rotation, Tile, TurnStep};

    use super::*;

    /// A room seating Red and Blue in `game`.
    fn room_playing(game: &Game) -> RoomData {
        let mut room = RoomData {
            match_id: 1,
            game: Some(game.to_data()),
            ..RoomData::default()
        };
        room.seats[Color::Red] = Some(SessionToken::new("red"));
        room.seats[Color::Blue] = Some(SessionToken::new("blue"));
        room
    }

    #[test]
    fn a_turn_ends_the_game_only_once_nobody_can_move() {
        let last_move = Move {
            tile: Tile::BYYBRR,
            position: Position::CENTER,
            rotation: Rotation::None,
            forced: false,
        };
        let players = EnumMap::from_fn(|c| match c {
            Color::Red | Color::Blue => Player::Someone,
            _ => Player::None,
        });
        let dealt = Game::from_players_seeded(players, 0).unwrap();
        let mut room = room_playing(&dealt);
        let change = room.finish_turn(dealt, last_move, &HashSet::new()).unwrap();
        assert!(!room.ended);
        assert!(matches!(change.message, HostMessage::StateUpdate { .. }));

        // Tiles are left in the bag, but no one holds any: the game stalls.
        let stalled = Game::from_snapshot(
            Board::default(),
            EnumMap::default(),
            10,
            Color::Red,
            [Color::Red, Color::Blue],
            TurnStep::BeforeFreeMove,
        )
        .unwrap();
        let mut room = room_playing(&stalled);
        let change = room
            .finish_turn(stalled, last_move, &HashSet::new())
            .unwrap();
        assert!(room.ended);
        let HostMessage::GameEnded { reason, .. } = change.message else {
            panic!("a stalled game ends");
        };
        assert_eq!(
            reason,
            EndReason::Completed {
                winners: vec![Color::Red, Color::Blue]
            }
        );
    }
}
