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

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;
    use tantrix_game::{Color, Position, Rotation};

    use super::{Hovered, TurnPhase, UiState, UiStateStoreExt, UiStateStoreImplExt};
    use crate::state::test_games::seeded;
    use crate::util::test_page::GamePage;

    #[test]
    fn the_status_bar_says_what_to_do_next() {
        let text = |phase: TurnPhase, forced| phase.status_text(Color::Blue, forced);
        assert_eq!(
            text(TurnPhase::SelectTile, false),
            "Select a tile from your hand"
        );
        assert_eq!(
            text(TurnPhase::SelectTile, true),
            "Select a tile — forced move!"
        );
        assert_eq!(
            text(TurnPhase::SelectPosition, true),
            "Click a position on the board"
        );
        assert_eq!(text(TurnPhase::SelectRotation, false), "Choose a rotation");
        assert_eq!(text(TurnPhase::WaitingForAi, false), "Blue is thinking...");
        assert_eq!(text(TurnPhase::GameOver, true), "Game Over!");
    }

    #[test]
    fn hovering_names_one_thing_at_a_time() {
        let at = Position::CENTER;
        assert_eq!(Hovered::HandTile(2).hand_tile(), Some(2));
        assert_eq!(Hovered::HandTile(2).board_position(), None);
        assert_eq!(Hovered::BoardPosition(at).board_position(), Some(at));
        assert_eq!(
            Hovered::Rotation(Rotation::R60).rotation(),
            Some(Rotation::R60)
        );
        assert_eq!(
            Hovered::ScoringColor(Color::Red).scoring_color(),
            Some(Color::Red)
        );
        assert_eq!(Hovered::None.rotation(), None);
    }

    /// The turn-flow fields of the page's UI state.
    fn turn(page: &GamePage) -> (TurnPhase, Option<usize>, Option<Position>, Hovered, String) {
        page.with(|ctx| {
            let ui = ctx.ui_state;
            (
                *ui.phase().peek(),
                *ui.selected_tile().peek(),
                *ui.selected_position().peek(),
                *ui.hovered().peek(),
                ui.message().peek().clone(),
            )
        })
    }

    #[test]
    fn going_back_steps_back_through_the_turn() {
        let at = Position::CENTER;
        let mut page = GamePage::local(seeded(3));
        page.act(|mut ctx| {
            ctx.ui_state.select_tile(1);
            ctx.ui_state.selected_position().set(Some(at));
            ctx.ui_state.phase().set(TurnPhase::SelectRotation);
            ctx.ui_state
                .hovered()
                .set(Hovered::Rotation(Rotation::R120));
        });

        // From the wheel back to the board: the tile stays picked.
        page.act(|mut ctx| ctx.ui_state.go_back());
        assert_eq!(
            turn(&page),
            (
                TurnPhase::SelectPosition,
                Some(1),
                None,
                Hovered::None,
                String::new()
            )
        );
        // From the board back to the hand.
        page.act(|mut ctx| ctx.ui_state.go_back());
        assert_eq!(
            turn(&page),
            (
                TurnPhase::SelectTile,
                None,
                None,
                Hovered::None,
                String::new()
            )
        );
        // Nowhere further back.
        page.act(|mut ctx| ctx.ui_state.go_back());
        assert_eq!(turn(&page).0, TurnPhase::SelectTile);
    }

    #[test]
    fn going_back_leaves_the_ai_and_the_results_alone() {
        let page = GamePage::local(seeded(3));
        for phase in [TurnPhase::WaitingForAi, TurnPhase::GameOver] {
            let after = page.with(|mut ctx| {
                ctx.ui_state.selected_tile().set(Some(0));
                ctx.ui_state.phase().set(phase);
                ctx.ui_state.go_back();
                (
                    *ctx.ui_state.phase().peek(),
                    *ctx.ui_state.selected_tile().peek(),
                )
            });
            assert_eq!(after, (phase, Some(0)));
        }
    }

    #[test]
    fn selecting_a_tile_starts_the_placement_afresh() {
        let mut page = GamePage::local(seeded(3));
        page.act(|ctx| {
            ctx.ui_state.message().set("Forced move!".to_string());
            ctx.ui_state.selected_position().set(Some(Position::CENTER));
            ctx.ui_state.hovered().set(Hovered::HandTile(4));
        });
        page.act(|mut ctx| ctx.ui_state.select_tile(4));
        assert_eq!(
            turn(&page),
            (
                TurnPhase::SelectPosition,
                Some(4),
                None,
                Hovered::None,
                String::new()
            )
        );
    }

    #[test]
    fn a_reset_returns_to_the_hand_with_its_message() {
        let mut page = GamePage::local(seeded(3));
        page.act(|mut ctx| ctx.ui_state.select_tile(2));
        page.act(|mut ctx| ctx.ui_state.reset_selection_with_message("Forced move!"));
        assert_eq!(
            turn(&page),
            (
                TurnPhase::SelectTile,
                None,
                None,
                Hovered::None,
                "Forced move!".to_string()
            )
        );
        page.act(|mut ctx| ctx.ui_state.reset_selection());
        assert_eq!(turn(&page).4, "");
    }

    #[test]
    fn a_move_recentres_the_board_and_marks_what_changed() {
        let mut page = GamePage::local(seeded(3));
        let at = Position::CENTER;
        page.act(|ctx| {
            ctx.ui_state.pan_x().set(12.0);
            ctx.ui_state.pan_y().set(-4.0);
            ctx.ui_state.zoom().set(2.0);
        });
        page.act(|mut ctx| ctx.ui_state.after_move(Color::Red, 3, at));
        let ui = page.with(|ctx| ctx.ui_state.cloned());
        assert_eq!(
            ui,
            UiState {
                // The zoom is the player's: a move keeps it.
                zoom: 2.0,
                new_tile: Some((Color::Red, 3)),
                last_placed_position: Some(at),
                ..UiState::default()
            }
        );
    }
}
