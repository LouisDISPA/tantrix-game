use std::collections::BTreeMap;

use dioxus::prelude::*;
use tantrix_game::{
    Bag, Board, Color, EnumMap, Hand, Move, Player, Position, Rotation, TurnTracker,
    TILES_IN_HAND,
};

use super::config::GameConfig;
use super::ui_state::UiState;

/// Context struct holding the game's reactive state.
/// Uses Store fields so child components get fine-grained field-level reactivity.
#[derive(Clone, Copy)]
pub struct GameContext {
    pub game_state: Store<GameState>,
    pub ui_state: Store<UiState>,
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

#[derive(Debug, Clone, Store)]
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
        let turn_tracker = TurnTracker::new(&players);

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

    pub fn refresh_hand_info(&mut self) {
        let current = self.turn_tracker.current();
        let all_moves = self.board.possible_moves(&self.hands[current], self.bag.is_empty());

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

    pub fn play_move(&mut self, m: Move) -> Result<(), String> {
        let current = self.turn_tracker.current();

        if !self.hands[current].contains(&m.tile) {
            return Err("Tile not in hand".to_string());
        }

        let has_forced_move = self
            .board
            .possible_moves(&self.hands[current], self.bag.is_empty())
            .iter()
            .any(|pm| pm.forced);
        let move_is_forced = self.board.possition_is_forced(&m.position);
        if has_forced_move && !move_is_forced {
            return Err("Must play a forced move".to_string());
        }

        self.board.insert_move(m)?;
        self.hands[current].play_tile(m.tile, &mut self.bag);
        if !move_is_forced {
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
        let possible = self.board.possible_moves(&self.hands[current], self.bag.is_empty());
        if possible.is_empty() {
            return None;
        }
        if let Player::Ai(ref ai) = self.players[current] {
            Some(ai.choose_move(&self.board, &self.hands, current, &possible))
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
