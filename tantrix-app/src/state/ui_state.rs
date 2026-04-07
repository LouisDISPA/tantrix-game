use dioxus::prelude::*;
use tantrix_game::{Color, Position, Rotation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnPhase {
    SelectTile,
    SelectPosition,
    SelectRotation,
    WaitingForAi,
    GameOver,
}

#[derive(Debug, Clone, PartialEq, Store)]
pub struct UiState {
    // --- Turn phase ---
    pub phase: TurnPhase,
    pub message: String,

    // --- Selection state ---
    pub selected_tile: Option<usize>,
    pub hovered_tile: Option<usize>,
    pub selected_position: Option<Position>,
    pub hovered_position: Option<Position>,
    pub selected_rotation: Option<Rotation>,
    pub hovered_rotation: Option<Rotation>,

    // --- Camera / viewport ---
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub dragging: bool,
    pub last_drag_x: f64,
    pub last_drag_y: f64,

    // --- Animation / transition ---
    /// The most recently drawn replacement tile: (player color, hand index).
    /// Used to animate only the new tile entering the hand.
    pub new_tile: Option<(Color, usize)>,
    /// Position of the most recently placed tile on the board (for placement animation).
    pub last_placed_position: Option<Position>,

    // --- Scoring overlay ---
    /// When Some, only this color's scoring path is highlighted on the board.
    pub highlighted_color: Option<Color>,
    /// Transiently highlights a color's path on hover.
    pub hovered_scoring_color: Option<Color>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            phase: TurnPhase::SelectTile,
            message: String::new(),

            selected_tile: None,
            hovered_tile: None,
            selected_position: None,
            hovered_position: None,
            selected_rotation: None,
            hovered_rotation: None,

            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            dragging: false,
            last_drag_x: 0.0,
            last_drag_y: 0.0,

            new_tile: None,
            last_placed_position: None,

            highlighted_color: None,
            hovered_scoring_color: None,
        }
    }
}

impl UiState {
    /// Clear all selection/hover state and return to SelectTile phase.
    pub fn reset_selection(&mut self) {
        self.selected_tile = None;
        self.hovered_tile = None;
        self.selected_position = None;
        self.hovered_position = None;
        self.selected_rotation = None;
        self.hovered_rotation = None;
        self.phase = TurnPhase::SelectTile;
        self.message.clear();
    }

    /// Navigate one step back in the turn flow.
    pub fn go_back(&mut self) {
        match self.phase {
            TurnPhase::SelectRotation => {
                self.selected_rotation = None;
                self.hovered_rotation = None;
                self.selected_position = None;
                self.phase = TurnPhase::SelectPosition;
            }
            TurnPhase::SelectPosition => {
                self.selected_position = None;
                self.selected_tile = None;
                self.phase = TurnPhase::SelectTile;
            }
            _ => {}
        }
    }

    /// Select a tile and enter position-selection phase.
    pub fn select_tile(&mut self, idx: usize) {
        self.selected_tile = Some(idx);
        self.selected_position = None;
        self.selected_rotation = None;
        self.hovered_position = None;
        self.phase = TurnPhase::SelectPosition;
        self.message.clear();
    }

    /// Record that a move was just played: recenter camera and mark new tile.
    pub fn after_move(&mut self, player_color: Color, tile_idx: usize, position: Position) {
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.new_tile = Some((player_color, tile_idx));
        self.last_placed_position = Some(position);
    }
}
