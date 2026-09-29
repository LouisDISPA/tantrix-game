use std::collections::BTreeMap;

use dioxus::prelude::*;
use tantrix_game::{
    play_move_parts, Bag, Board, Color, EnumMap, Hand, Move, MoveError, Player, PlayerScore,
    Position, Rotation, TurnTracker, TILES_IN_HAND,
};
use tantrix_proto::GameSnapshot;

use super::config::GameConfig;
use super::online_state::CommandSender;
use super::ui_state::{Screen, UiState};

/// The single context shared across every component in the local-game page.
///
/// * `screen` — which top-level screen is showing (Setup / Playing)
/// * `game_state` — the atomic game snapshot; components read it wholesale
/// * `ui_state` — per-field reactive UI state (zoom, hover, selection, …)
/// * `online` — if set, `actions::execute_move` forwards the move to the
///   host instead of mutating the local `game_state`. Local games leave
///   this as `None`.
/// * `my_color` — if `Some(c)`, only this color's hand is interactive on
///   this device. Online games set this from the lobby seat; local games
///   leave it `None` so every current-turn hand is clickable.
/// * `player_names` — optional nicknames per color, shown beside the color
///   in each hand's header. Empty for local games.
#[derive(Clone, Copy)]
pub struct GameCtx {
    pub screen: Signal<Screen>,
    pub game_state: Signal<GameState>,
    pub ui_state: Store<UiState>,
    pub online: Signal<Option<CommandSender>>,
    pub my_color: Signal<Option<Color>>,
    pub player_names: Signal<EnumMap<Color, Option<String>>>,
}

#[derive(Debug, Clone)]
pub struct TileMoveInfo {
    pub has_forced: bool,
    pub positions: BTreeMap<Position, Vec<Rotation>>,
}

impl TileMoveInfo {
    pub fn has_moves(&self) -> bool {
        !self.positions.is_empty()
    }
}

/// Core game data. `GameState` is treated as one atomic unit — every mutation
/// (`play_move`, `skip_turn`, …) updates multiple fields together, so there's
/// nothing to gain from per-field reactivity. Providers wrap this in a plain
/// `Signal<GameState>`.
#[derive(Debug, Clone)]
pub struct GameState {
    pub board: Board,
    pub bag: Bag,
    pub hands: EnumMap<Color, Hand>,
    pub players: EnumMap<Color, Player>,
    pub turn_tracker: TurnTracker,
    pub hand_info: Vec<TileMoveInfo>,
}

impl GameState {
    pub fn new(config: &GameConfig) -> Self {
        let players = EnumMap::from_fn(|color| config.player_for(color));

        let mut bag = Bag::new();
        let turn_tracker = TurnTracker::new(&players)
            .expect("GameConfig always creates at least two active players");

        let mut hands: EnumMap<Color, Hand> = Default::default();
        for &color in turn_tracker.playing_colors() {
            hands[color].deal(&mut bag, TILES_IN_HAND);
        }

        let mut state = GameState {
            board: Board::default(),
            bag,
            hands,
            players,
            turn_tracker,
            hand_info: Vec::new(),
        };
        state.refresh_hand_info();
        state
    }

    /// Rebuild from an authoritative snapshot received over the wire. The
    /// bag's contents are stubbed (only its count matters for the UI), and
    /// each seated color is mapped to `Player::Someone`.
    pub fn from_snapshot(snap: &GameSnapshot) -> Self {
        let players: EnumMap<Color, Player> = EnumMap::from_fn(|c| {
            if snap.seats[c].is_some() {
                Player::Someone
            } else {
                Player::None
            }
        });

        let mut state = GameState {
            board: snap.board.clone(),
            bag: Bag::with_count(snap.bag_count),
            hands: snap.hands.clone(),
            players,
            turn_tracker: TurnTracker::from_parts(
                snap.current_player,
                snap.playing_colors.iter().copied(),
            )
            .expect("authoritative snapshots always include the current player"),
            hand_info: Vec::new(),
        };
        state.refresh_hand_info();
        state
    }

    pub fn refresh_hand_info(&mut self) {
        let current = self.turn_tracker.current();
        let all_moves = self
            .board
            .possible_moves(&self.hands[current], self.bag.is_empty());

        self.hand_info = self.hands[current]
            .iter()
            .map(|tile| {
                let tile_moves: Vec<&Move> = all_moves.iter().filter(|m| m.tile == *tile).collect();
                let has_forced = tile_moves.iter().any(|m| m.forced);

                let mut positions: BTreeMap<Position, Vec<Rotation>> = BTreeMap::new();
                for m in &tile_moves {
                    positions.entry(m.position).or_default().push(m.rotation);
                }

                TileMoveInfo {
                    has_forced,
                    positions,
                }
            })
            .collect();
    }

    pub fn play_move(&mut self, m: Move) -> Result<(), MoveError> {
        let current = self.turn_tracker.current();
        let played = play_move_parts(&mut self.board, &mut self.hands[current], &mut self.bag, m)?;
        if !played.m.forced {
            self.turn_tracker.next();
        }
        self.refresh_hand_info();
        Ok(())
    }

    pub fn skip_turn(&mut self) {
        self.turn_tracker.next();
        self.refresh_hand_info();
    }

    /// Skip past any players with no valid moves. Stops at a player who can act or game over.
    pub fn skip_stuck_players(&mut self) {
        let num_players = self.turn_tracker.playing_colors().len();
        for _ in 0..num_players {
            if self.is_game_over() || self.has_any_moves() {
                return;
            }
            self.turn_tracker.next();
            self.refresh_hand_info();
        }
    }

    /// If the current player is AI, compute its chosen move.
    pub fn ai_choose(&self) -> Option<Move> {
        if self.is_game_over() {
            return None;
        }
        let current = self.turn_tracker.current();
        let possible = self
            .board
            .possible_moves(&self.hands[current], self.bag.is_empty());
        if possible.is_empty() {
            return None;
        }
        if let Player::Ai(ref ai) = self.players[current] {
            Some(ai.choose_move(
                &self.board,
                &self.hands,
                current,
                self.turn_tracker.playing_colors(),
                &possible,
            ))
        } else {
            None
        }
    }

    pub fn is_game_over(&self) -> bool {
        self.bag.is_empty() && self.hands.values().all(|h| h.is_empty())
    }

    pub fn has_any_moves(&self) -> bool {
        self.hand_info.iter().any(|info| info.has_moves())
    }
}

/// Determine the winner(s) of a finished game from player scores.
/// Returns the list of players tied for the highest score and whether it's a tie.
pub fn determine_winners(
    active_players: &[Color],
    scores: &EnumMap<Color, PlayerScore>,
) -> (Vec<Color>, bool) {
    let max_score = active_players
        .iter()
        .map(|&c| scores[c].score)
        .max()
        .unwrap_or(0);
    let winners: Vec<Color> = active_players
        .iter()
        .copied()
        .filter(|&c| scores[c].score == max_score)
        .collect();
    let is_tie = winners.len() > 1;
    (winners, is_tie)
}
