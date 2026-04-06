use std::collections::BTreeMap;

use dioxus::prelude::*;
use tantrix_game::{Color, Game, Move, Position, Rotation, Tile};

use super::config::GameConfig;
use super::ui_state::UiState;

/// Context struct holding the game's reactive state.
/// Uses Signal fields so child components get fine-grained reactivity.
#[derive(Clone, Copy)]
pub struct GameContext {
    pub game_state: Signal<GameState>,
    pub ui_state: Signal<UiState>,
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

#[derive(Debug, Clone)]
pub struct GameState {
    pub game: Game,
    pub hand_info: Vec<TileMoveInfo>,
}

impl GameState {
    pub fn new(config: &GameConfig) -> Self {
        let game = Game::new(
            config.player_for(Color::Red),
            config.player_for(Color::Blue),
            config.player_for(Color::Yellow),
            config.player_for(Color::Green),
        );
        let mut state = GameState {
            game,
            hand_info: Vec::new(),
        };
        state.refresh_hand_info();
        state
    }

    pub fn refresh_hand_info(&mut self) {
        let all_moves = self.game.get_possible_moves().to_vec();
        let hand: Vec<Tile> = self.game.get_hand(self.game.currently_playing()).to_vec();

        self.hand_info = hand
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

    pub fn currently_playing(&self) -> Color {
        self.game.currently_playing()
    }

    pub fn play_move(&mut self, m: Move) -> Result<(), String> {
        self.game.play_move(m)?;
        self.refresh_hand_info();
        Ok(())
    }

    pub fn skip_turn(&mut self) {
        self.game.skip_turn();
        self.refresh_hand_info();
    }

    /// Skip past any players with no valid moves. Stops at a player who can act or game over.
    pub fn skip_stuck_players(&mut self) {
        let num_players = self.game.playing_colors().len();
        for _ in 0..num_players {
            if self.game.is_game_over() || self.has_any_moves() {
                return;
            }
            self.game.skip_turn();
            self.refresh_hand_info();
        }
    }

    /// If the current player is AI, compute its chosen move.
    pub fn ai_choose(&mut self) -> Option<Move> {
        self.game.ai_choose()
    }

    /// Find the index of a tile in the current player's hand.
    pub fn tile_index(&self, tile: Tile) -> Option<usize> {
        self.game
            .get_hand(self.game.currently_playing())
            .iter()
            .position(|t| *t == tile)
    }

    pub fn is_game_over(&self) -> bool {
        self.game.is_game_over()
    }

    pub fn has_any_moves(&self) -> bool {
        self.hand_info.iter().any(|info| info.has_moves())
    }

    pub fn has_forced_moves(&self) -> bool {
        self.hand_info.iter().any(|info| info.has_forced)
    }

    pub fn get_hand(&self, color: Color) -> &[Tile] {
        self.game.get_hand(color)
    }

    pub fn playing_colors(&self) -> &[Color] {
        self.game.playing_colors()
    }

    pub fn board_moves(&self) -> &[Move] {
        self.game.board().moves()
    }

    pub fn bag_count(&self) -> usize {
        self.game.bag_count()
    }

    /// Build a Move from the selected tile, position, and rotation.
    pub fn build_move(
        &self,
        tile_index: usize,
        position: Position,
        rotation: Rotation,
    ) -> Option<Move> {
        let hand = self.game.get_hand(self.game.currently_playing());
        let tile = *hand.get(tile_index)?;
        let forced = self
            .hand_info
            .get(tile_index)
            .map(|info| info.has_forced)
            .unwrap_or(false);

        Some(Move {
            tile,
            position,
            rotation,
            forced,
        })
    }
}
