use tantrix_game::{Color, Position, Rotation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnPhase {
    SelectTile,
    SelectPosition,
    SelectRotation,
    WaitingForAi,
    GameOver,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiState {
    pub phase: TurnPhase,
    pub selected_tile: Option<usize>,
    pub hovered_tile: Option<usize>,
    pub selected_position: Option<Position>,
    pub hovered_position: Option<Position>,
    pub selected_rotation: Option<Rotation>,
    pub hovered_rotation: Option<Rotation>,
    pub message: String,
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub dragging: bool,
    pub last_drag_x: f64,
    pub last_drag_y: f64,
    /// When true, the board SVG group animates its transform (smooth recenter).
    pub animating: bool,
    /// The most recently drawn replacement tile: (player color, hand index).
    /// Used to animate only the new tile entering the hand.
    pub new_tile: Option<(Color, usize)>,
    /// Position of the most recently placed tile on the board (for placement animation).
    pub last_placed_position: Option<Position>,
    /// When Some, only this color's scoring path is highlighted on the board.
    pub highlighted_color: Option<Color>,
    /// Transiently highlights a color's path on hover.
    pub hovered_scoring_color: Option<Color>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            phase: TurnPhase::SelectTile,
            selected_tile: None,
            hovered_tile: None,
            selected_position: None,
            hovered_position: None,
            selected_rotation: None,
            hovered_rotation: None,
            message: String::new(),
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            dragging: false,
            last_drag_x: 0.0,
            last_drag_y: 0.0,
            animating: false,
            new_tile: None,
            last_placed_position: None,
            highlighted_color: None,
            hovered_scoring_color: None,
        }
    }
}

impl UiState {
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
}
