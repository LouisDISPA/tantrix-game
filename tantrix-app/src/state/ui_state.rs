use tantrix_game::{Position, Rotation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnPhase {
    SelectTile,
    SelectPosition,
    SelectRotation,
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
    pub message: String,
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub dragging: bool,
    pub last_drag_x: f64,
    pub last_drag_y: f64,
    /// When true, the board SVG group animates its transform (smooth recenter).
    pub animating: bool,
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
            message: String::new(),
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            dragging: false,
            last_drag_x: 0.0,
            last_drag_y: 0.0,
            animating: false,
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
        self.phase = TurnPhase::SelectTile;
        self.message.clear();
    }

    pub fn go_back(&mut self) {
        match self.phase {
            TurnPhase::SelectRotation => {
                self.selected_rotation = None;
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
