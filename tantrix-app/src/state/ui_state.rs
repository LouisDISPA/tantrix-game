use dioxus::prelude::*;
use tantrix_game::{Color, Position, Rotation};

/// Which top-level screen inside the local-game page is showing. Orthogonal
/// to `TurnPhase`: Setup shows the new-game form, Playing delegates to the
/// active or game-over screen based on `TurnPhase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Setup,
    Playing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnPhase {
    SelectTile,
    SelectPosition,
    SelectRotation,
    WaitingForAi,
    GameOver,
}

impl TurnPhase {
    /// Human-readable banner text for the status bar. `current` is the player
    /// whose turn it is (used for the waiting-for-AI message).
    pub fn status_text(&self, current: Color, has_forced: bool) -> String {
        match self {
            Self::SelectTile if has_forced => "Select a tile — forced move!".to_string(),
            Self::SelectTile => "Select a tile from your hand".to_string(),
            Self::SelectPosition => "Click a position on the board".to_string(),
            Self::SelectRotation => "Choose a rotation".to_string(),
            Self::WaitingForAi => format!("{} is thinking...", current),
            Self::GameOver => "Game Over!".to_string(),
        }
    }
}

/// The single thing the user is hovering over. At any moment the user can only
/// be pointing at one element, so the four former hovered_* fields are unified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hovered {
    #[default]
    None,
    HandTile(usize),
    BoardPosition(Position),
    Rotation(Rotation),
    ScoringColor(Color),
}

impl Hovered {
    pub fn hand_tile(&self) -> Option<usize> {
        match self {
            Self::HandTile(i) => Some(*i),
            _ => None,
        }
    }
    pub fn board_position(&self) -> Option<Position> {
        match self {
            Self::BoardPosition(p) => Some(*p),
            _ => None,
        }
    }
    pub fn rotation(&self) -> Option<Rotation> {
        match self {
            Self::Rotation(r) => Some(*r),
            _ => None,
        }
    }
    pub fn scoring_color(&self) -> Option<Color> {
        match self {
            Self::ScoringColor(c) => Some(*c),
            _ => None,
        }
    }
}

/// Whether the board viewport is currently being panned, and (if so) where
/// the last pointer-move event landed so we can compute a delta.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Drag {
    #[default]
    Idle,
    Active {
        last_x: f64,
        last_y: f64,
    },
}

impl Drag {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }
}

/// Primary visual state for a single tile in a player's hand.
/// Variants are mutually exclusive — highest-priority one wins when multiple
/// conditions apply (Selected beats Forced beats Disabled beats Idle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HandTileState {
    /// Playable tile with no special state.
    #[default]
    Idle,
    /// Cannot be played right now (no moves, or another tile is forced).
    Disabled,
    /// A forced move is available for this tile.
    Forced,
    /// This tile is currently selected by the user.
    Selected,
}

impl HandTileState {
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Idle => "",
            Self::Disabled => "disabled",
            Self::Forced => "forced",
            Self::Selected => "selected",
        }
    }

    pub fn is_interactive(&self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

#[derive(Debug, Clone, PartialEq, Store)]
pub struct UiState {
    // --- Turn phase ---
    pub phase: TurnPhase,
    pub message: String,

    // --- Selection state ---
    pub selected_tile: Option<usize>,
    pub selected_position: Option<Position>,

    // --- Hover (unified) ---
    pub hovered: Hovered,

    // --- Camera / viewport ---
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub drag: Drag,

    // --- Animation / transition ---
    /// The most recently drawn replacement tile: (player color, hand index).
    /// Used to animate only the new tile entering the hand.
    pub new_tile: Option<(Color, usize)>,
    /// Position of the most recently placed tile on the board (for placement animation).
    pub last_placed_position: Option<Position>,

    // --- Scoring overlay (persistent click-to-lock) ---
    /// When Some, only this color's scoring path is highlighted on the board.
    pub highlighted_color: Option<Color>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            phase: TurnPhase::SelectTile,
            message: String::new(),

            selected_tile: None,
            selected_position: None,

            hovered: Hovered::None,

            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            drag: Drag::Idle,

            new_tile: None,
            last_placed_position: None,

            highlighted_color: None,
        }
    }
}

impl UiState {
    /// Clear all selection/hover state and return to SelectTile phase.
    pub fn reset_selection(&mut self) {
        self.selected_tile = None;
        self.selected_position = None;
        self.hovered = Hovered::None;
        self.phase = TurnPhase::SelectTile;
        self.message.clear();
    }

    /// Navigate one step back in the turn flow.
    pub fn go_back(&mut self) {
        match self.phase {
            TurnPhase::SelectRotation => {
                self.selected_position = None;
                self.hovered = Hovered::None;
                self.phase = TurnPhase::SelectPosition;
            }
            TurnPhase::SelectPosition => {
                self.selected_position = None;
                self.selected_tile = None;
                self.hovered = Hovered::None;
                self.phase = TurnPhase::SelectTile;
            }
            _ => {}
        }
    }

    /// Select a tile and enter position-selection phase.
    pub fn select_tile(&mut self, idx: usize) {
        self.selected_tile = Some(idx);
        self.selected_position = None;
        self.hovered = Hovered::None;
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
