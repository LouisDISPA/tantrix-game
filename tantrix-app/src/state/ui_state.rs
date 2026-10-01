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

/// The turn flow's multi-field changes to the UI state.
///
/// Each writes field by field, and only the fields whose value changes: a
/// write through the whole store (`ui_state.write()`) marks *every* field
/// dirty — the camera, the animation hints, the scoring overlay — and wakes
/// all of their readers for nothing.
#[store(pub)]
impl<Lens> Store<UiState, Lens> {
    /// Clear all selection/hover state and return to SelectTile phase.
    fn reset_selection(&mut self) {
        self.reset_selection_with_message("");
    }

    /// [`reset_selection`](UiStateStoreImplExt::reset_selection), then show
    /// `message` in the status bar (none when empty).
    fn reset_selection_with_message(&mut self, message: &str) {
        self.clear_selection();
        set_if_changed(self.phase(), TurnPhase::SelectTile);
        let stale = *self.message().peek() != message;
        if stale {
            self.message().set(message.to_string());
        }
    }

    /// Forget the selected tile and position and what is hovered; the phase
    /// and message stay as they are.
    fn clear_selection(&mut self) {
        set_if_changed(self.selected_tile(), None);
        set_if_changed(self.selected_position(), None);
        set_if_changed(self.hovered(), Hovered::None);
    }

    /// Navigate one step back in the turn flow.
    fn go_back(&mut self) {
        // Copy the phase out: a read guard held across the writes below
        // would conflict with them.
        let phase = *self.phase().peek();
        match phase {
            TurnPhase::SelectRotation => {
                set_if_changed(self.selected_position(), None);
                set_if_changed(self.hovered(), Hovered::None);
                self.phase().set(TurnPhase::SelectPosition);
            }
            TurnPhase::SelectPosition => {
                self.clear_selection();
                self.phase().set(TurnPhase::SelectTile);
            }
            _ => {}
        }
    }

    /// Select a tile and enter position-selection phase.
    fn select_tile(&mut self, idx: usize) {
        set_if_changed(self.selected_tile(), Some(idx));
        set_if_changed(self.selected_position(), None);
        set_if_changed(self.hovered(), Hovered::None);
        set_if_changed(self.phase(), TurnPhase::SelectPosition);
        set_if_changed(self.message(), String::new());
    }

    /// Record that a move was just played: recenter camera and mark new tile.
    fn after_move(&mut self, player_color: Color, tile_idx: usize, position: Position) {
        set_if_changed(self.pan_x(), 0.0);
        set_if_changed(self.pan_y(), 0.0);
        set_if_changed(self.new_tile(), Some((player_color, tile_idx)));
        set_if_changed(self.last_placed_position(), Some(position));
    }
}

/// Write `value` into one reactive field unless it already holds it, so the
/// field's readers are only woken by a real change (a write marks them dirty
/// whatever the value).
pub fn set_if_changed<T, F>(mut field: F, value: T)
where
    T: PartialEq + 'static,
    F: Readable<Target = T> + Writable<Target = T>,
{
    let changed = *field.peek() != value;
    if changed {
        field.set(value);
    }
}
