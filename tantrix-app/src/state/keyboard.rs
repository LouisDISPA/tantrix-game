//! Keyboard play: what the keys do at each step of a turn, and where the
//! keyboard's focus goes next.
//!
//! A turn has three steps — a tile from the hand, a spot on the board, a way
//! round on the card of options — and the keys work alike at each: the
//! arrow keys move between the choices, Enter (or Space) takes one, and
//! Escape steps back (`GamePlay` hears it from anywhere on the game screen).
//! Each choice the keyboard takes moves the focus on to the next step's
//! choices; a move played sends it to the hand of the next seat this device
//! plays, once that seat is to move.
//!
//! The board's spots are a single tab stop (`BoardSpots`: a listbox whose
//! active option is its `aria-activedescendant`, so focus never moves onto
//! the transformed hexes). A hex board is no grid, so the spots are read
//! like text ([`spot_order`]): row by row from the top, left to right in a
//! row. ← and → step through them in that order, round from the last to the
//! first; ↑ and ↓ go to the nearest spot in the closest row above or below
//! that has one; Home and End go to the first and the last. The active spot
//! is the hovered position (`Hovered::BoardPosition`), so the keys show the
//! same ghost tile as the mouse.
//!
//! The functions taking a `GameCtx` are the key handlers' actions; the rest
//! are the pure decisions they make.

use dioxus::prelude::*;
use tantrix_game::{Color, Position, Rotation, TileMoveInfo};

use crate::hooks::camera::BoardCamera;
use crate::state::actions::{click_position, click_tile, pick_rotation, placed_positions};
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{
    set_if_changed, Hovered, TurnPhase, UiStateStoreExt, UiStateStoreImplExt,
};

/// Where the keyboard's focus should go next. A key handler asks for it
/// (`UiState::focus_request`); the component that shows the target takes the
/// focus once it can, and clears the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusTarget {
    /// A tile of the hand to move, on this device: this one if it can be
    /// played, else the first that can.
    Hand(Option<usize>),
    /// The first playable tile of the hand to move once a move has been
    /// played after this one — the board holds more than `placed` tiles —
    /// and a seat this device plays is to choose a tile. Online, that waits
    /// for the room's snapshot, and for the other players' turns.
    NextTurn { placed: usize },
    /// The board's spots (`BoardSpots`).
    Spots,
    /// The card of options' button at this index.
    Card(usize),
}

impl FocusTarget {
    /// The index of the hand tile that takes this focus now, in a hand of
    /// which `playable` says which tiles can be played (the hand to move's,
    /// on a device that plays it) at `phase`, with `placed` tiles on the
    /// board. `None` when the focus is not for the hand, or not yet.
    pub(crate) fn hand_tile(
        self,
        phase: TurnPhase,
        placed: usize,
        playable: &[bool],
    ) -> Option<usize> {
        let wanted = match self {
            Self::Hand(at) if is_choosing(phase) => at,
            Self::NextTurn { placed: before }
                if phase == TurnPhase::SelectTile && placed > before =>
            {
                None
            }
            _ => return None,
        };
        hand_focus(playable, wanted)
    }
}

/// Whether `phase` is one of the three steps a player takes.
fn is_choosing(phase: TurnPhase) -> bool {
    matches!(
        phase,
        TurnPhase::SelectTile | TurnPhase::SelectPosition | TurnPhase::SelectRotation
    )
}

/// A move between the choices of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Nav {
    Prev,
    Next,
    Up,
    Down,
    First,
    Last,
}

impl Nav {
    /// The move `key` asks for: the arrow keys, Home and End. `None` for any
    /// other key, and for any key held with Ctrl, Alt or Meta — those are
    /// the browser's and the system's shortcuts.
    pub(crate) fn of(key: &Key, modifiers: Modifiers) -> Option<Self> {
        if modifiers.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META) {
            return None;
        }
        match key {
            Key::Named(NamedKey::ArrowLeft) => Some(Self::Prev),
            Key::Named(NamedKey::ArrowRight) => Some(Self::Next),
            Key::Named(NamedKey::ArrowUp) => Some(Self::Up),
            Key::Named(NamedKey::ArrowDown) => Some(Self::Down),
            Key::Named(NamedKey::Home) => Some(Self::First),
            Key::Named(NamedKey::End) => Some(Self::Last),
            _ => None,
        }
    }

    /// The move `key` asks for on the card of options, a single row: any
    /// arrow steps along it, and R turns the tile on (Shift+R back).
    pub(crate) fn of_rotation(key: &Key, modifiers: Modifiers) -> Option<Self> {
        if let Key::Character(c) = key {
            let plain =
                !modifiers.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META);
            if plain && c.eq_ignore_ascii_case("r") {
                return Some(if modifiers.contains(Modifiers::SHIFT) {
                    Self::Prev
                } else {
                    Self::Next
                });
            }
        }
        Self::of(key, modifiers).map(|nav| match nav {
            Self::Up => Self::Prev,
            Self::Down => Self::Next,
            other => other,
        })
    }
}

/// Whether `key` takes the choice in focus: Enter or Space, not held with
/// Ctrl, Alt or Meta.
pub(crate) fn is_choice(key: &Key, modifiers: Modifiers) -> bool {
    !modifiers.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
        && (matches!(key, Key::Named(NamedKey::Enter))
            || matches!(key, Key::Character(c) if c == " "))
}

/// Whether the player is on the keys (`UiState::keyboard`): a focus then
/// previews, and a click moves the focus on, as a key would.
pub(crate) fn keys_in_use(ctx: GameCtx) -> bool {
    *ctx.ui_state.keyboard().peek()
}

/// Where `nav` goes in a row of `len` choices from the one at `at` (none:
/// the row's start): ← and → round from one end to the other, Home and End
/// to the ends. `None` for ↑ and ↓, which leave a row, and in an empty row.
pub(crate) fn step_in(len: usize, at: Option<usize>, nav: Nav) -> Option<usize> {
    let last = len.checked_sub(1)?;
    Some(match (nav, at) {
        (Nav::First, _) | (Nav::Next, None) => 0,
        (Nav::Last, _) | (Nav::Prev, None) => last,
        (Nav::Next, Some(at)) => (at + 1) % len,
        (Nav::Prev, Some(at)) => at.checked_sub(1).unwrap_or(last),
        (Nav::Up | Nav::Down, _) => return None,
    })
}

/// A spot's place across the board, in half hexes: its pixel column.
fn column(at: Position) -> i32 {
    2 * i32::from(at.x()) - i32::from(at.y())
}

/// Where `at` comes as the keys read the board: by row from the top of the
/// screen (the game's y grows upwards), then left to right.
pub(crate) fn reading_key(at: Position) -> (i32, i32) {
    (-i32::from(at.y()), column(at))
}

/// `spots` in [`reading_key`] order.
pub(crate) fn spot_order(spots: impl IntoIterator<Item = Position>) -> Vec<Position> {
    let mut order: Vec<Position> = spots.into_iter().collect();
    order.sort_by_key(|&at| reading_key(at));
    order
}

/// The spot `nav` goes to from `from` among `order` (read in
/// [`spot_order`]). With no spot active yet, the first (the last for ← and
/// End). ↑ and ↓ keep to the spot when no row above or below has one.
pub(crate) fn step_spot(order: &[Position], from: Option<Position>, nav: Nav) -> Option<Position> {
    let at = from.and_then(|from| order.iter().position(|&spot| spot == from));
    let from = match (at, nav) {
        (Some(at), Nav::Up | Nav::Down) => order[at],
        (None, Nav::Up | Nav::Down) => return order.first().copied(),
        _ => return step_in(order.len(), at, nav).map(|i| order[i]),
    };
    // The closest row above (a greater y) or below that has a spot.
    let row = if nav == Nav::Up {
        order.iter().map(|s| s.y()).filter(|&y| y > from.y()).min()
    } else {
        order.iter().map(|s| s.y()).filter(|&y| y < from.y()).max()
    };
    let Some(row) = row else { return Some(from) };
    // Its spot nearest across; on a tie, the left one (read first).
    order
        .iter()
        .copied()
        .filter(|spot| spot.y() == row)
        .min_by_key(|&spot| (column(spot) - column(from)).abs())
}

/// Which of the hand to move's tiles can be played: those with a move.
/// While a forced space can be filled the rules offer only the moves that
/// fill one, so the other tiles have none.
pub(crate) fn playable_tiles(infos: &[TileMoveInfo]) -> Vec<bool> {
    infos.iter().map(TileMoveInfo::has_moves).collect()
}

/// The tile the hand's focus goes to: `wanted` if it can be played, else
/// the first that can.
pub(crate) fn hand_focus(playable: &[bool], wanted: Option<usize>) -> Option<usize> {
    wanted
        .filter(|&at| playable.get(at) == Some(&true))
        .or_else(|| playable.iter().position(|&can| can))
}

/// The playable tile `nav` goes to from the tile at `from` (← and → round
/// the hand, Home and End), skipping the tiles that cannot be played.
pub(crate) fn step_tile(playable: &[bool], from: usize, nav: Nav) -> Option<usize> {
    let tiles: Vec<usize> = (0..playable.len()).filter(|&i| playable[i]).collect();
    let at = tiles.iter().position(|&i| i == from);
    step_in(tiles.len(), at, nav).map(|i| tiles[i])
}

/// How the card of options names a way round: as the tile is turned from
/// the way the hand shows it (a rotation turns it clockwise on screen).
pub(crate) fn rotation_label(rotation: Rotation) -> &'static str {
    match rotation {
        Rotation::None => "Place it as in your hand",
        Rotation::R60 => "Place it turned 60° clockwise",
        Rotation::R120 => "Place it turned 120° clockwise",
        Rotation::R180 => "Place it turned 180°",
        Rotation::R240 => "Place it turned 120° anticlockwise",
        Rotation::R300 => "Place it turned 60° anticlockwise",
    }
}

/// The keyboard's place at each step: the hand, the spots, the card.
pub(crate) fn step_focus(phase: TurnPhase) -> Option<FocusTarget> {
    match phase {
        TurnPhase::SelectTile => Some(FocusTarget::Hand(None)),
        TurnPhase::SelectPosition => Some(FocusTarget::Spots),
        TurnPhase::SelectRotation => Some(FocusTarget::Card(0)),
        TurnPhase::WaitingForAi | TurnPhase::GameOver => None,
    }
}

/// Ask for the keyboard's focus to go to `target`.
pub(crate) fn request_focus(ctx: GameCtx, target: FocusTarget) {
    set_if_changed(ctx.ui_state.focus_request(), Some(target));
}

/// The board's tile count now.
fn placed(ctx: GameCtx) -> usize {
    ctx.game_state.peek().board().len()
}

/// The spots of the selected tile, in [`spot_order`].
pub(crate) fn selected_spots(ctx: GameCtx) -> Vec<Position> {
    let Some(idx) = *ctx.ui_state.selected_tile().peek() else {
        return Vec::new();
    };
    let gs = ctx.game_state.peek();
    gs.hand_info()
        .get(idx)
        .map(|info| spot_order(info.positions.keys().copied()))
        .unwrap_or_default()
}

/// The spot the keys are on: the hovered position, when it is a spot.
fn active_spot(ctx: GameCtx, spots: &[Position]) -> Option<Position> {
    ctx.ui_state
        .hovered()
        .peek()
        .board_position()
        .filter(|at| spots.contains(at))
}

/// Make `at` the spot the keys are on (and the ghost's), and keep it on
/// the stage.
fn activate_spot(ctx: GameCtx, at: Position) {
    set_if_changed(ctx.ui_state.hovered(), Hovered::BoardPosition(at));
    BoardCamera::new(ctx.ui_state).frame_spots(&placed_positions(ctx), &[at]);
}

/// The spots took the keyboard's focus: the keys go on from the spot
/// already hovered (the one just stepped back from), else the one whose
/// card is open, else the first.
pub(crate) fn focus_spots(ctx: GameCtx) {
    let spots = selected_spots(ctx);
    let open = (*ctx.ui_state.selected_position().peek()).filter(|at| spots.contains(at));
    let at = active_spot(ctx, &spots)
        .or(open)
        .or_else(|| spots.first().copied());
    if let Some(at) = at {
        activate_spot(ctx, at);
    }
}

/// An arrow key, Home or End on the spots.
pub(crate) fn move_spot(ctx: GameCtx, nav: Nav) {
    let spots = selected_spots(ctx);
    if let Some(at) = step_spot(&spots, active_spot(ctx, &spots), nav) {
        activate_spot(ctx, at);
    }
}

/// Enter on the spots: take the active spot, as a click would. A spot that
/// takes the tile two ways opens the card, which takes the focus; a move
/// played sends it on to the next turn.
pub(crate) fn choose_spot(ctx: GameCtx) {
    let spots = selected_spots(ctx);
    let Some(at) = active_spot(ctx, &spots) else {
        return;
    };
    let before = placed(ctx);
    click_position(ctx, at);
    after_choice(ctx, before);
}

/// Enter on `color`'s hand tile at `idx`: as a click, then the focus on to
/// the spots, or to the next turn when the tile played itself. Enter on the
/// selected tile puts it back; the focus stays, previewing it again.
pub(crate) fn choose_tile(ctx: GameCtx, color: Color, idx: usize) {
    let was_selected = is_choosing(*ctx.ui_state.phase().peek())
        && *ctx.ui_state.selected_tile().peek() == Some(idx);
    let before = placed(ctx);
    click_tile(ctx, color, idx);
    if was_selected {
        set_if_changed(ctx.ui_state.hovered(), Hovered::HandTile(idx));
        request_focus(ctx, FocusTarget::Hand(Some(idx)));
    } else {
        after_choice(ctx, before);
    }
}

/// An arrow key, Home or End on the hand tile at `idx`: the focus goes to
/// the playable tile it points at.
pub(crate) fn step_hand(ctx: GameCtx, idx: usize, nav: Nav) {
    let playable = playable_tiles(ctx.game_state.peek().hand_info());
    if let Some(next) = step_tile(&playable, idx, nav) {
        request_focus(ctx, FocusTarget::Hand(Some(next)));
    }
}

/// Enter on an option of the card: play it, and the focus on to the next
/// turn.
pub(crate) fn choose_rotation(ctx: GameCtx, rotation: Rotation) {
    let before = placed(ctx);
    pick_rotation(ctx, rotation);
    after_choice(ctx, before);
}

/// After a choice the keyboard took, with `before` tiles on the board: the
/// focus goes to the step it led to — or, once the move is played, to the
/// next turn. A choice that changed nothing leaves it.
fn after_choice(ctx: GameCtx, before: usize) {
    // Copied out: a read guard held across the write below would conflict.
    let phase = *ctx.ui_state.phase().peek();
    match phase {
        TurnPhase::SelectPosition => request_focus(ctx, FocusTarget::Spots),
        TurnPhase::SelectRotation => request_focus(ctx, FocusTarget::Card(0)),
        TurnPhase::SelectTile | TurnPhase::WaitingForAi | TurnPhase::GameOver => {
            let unselected = ctx.ui_state.selected_tile().peek().is_none();
            if placed(ctx) > before || unselected {
                request_focus(ctx, FocusTarget::NextTurn { placed: before });
            }
        }
    }
}

/// Escape: one step back through the turn, and the focus with it — from the
/// card to its spot, from the spots to the tile.
pub(crate) fn step_back(ctx: GameCtx) {
    let mut ui = ctx.ui_state;
    let phase = *ui.phase().peek();
    let tile = *ui.selected_tile().peek();
    let spot = *ui.selected_position().peek();
    ui.go_back();
    match phase {
        TurnPhase::SelectRotation => {
            if let Some(spot) = spot {
                set_if_changed(ui.hovered(), Hovered::BoardPosition(spot));
            }
            request_focus(ctx, FocusTarget::Spots);
        }
        TurnPhase::SelectPosition => request_focus(ctx, FocusTarget::Hand(tile)),
        _ => {}
    }
}

/// A key on the game screen itself, nothing in it focused: the turn's step
/// takes the focus (on this device's turn).
pub(crate) fn enter_turn(ctx: GameCtx) {
    let mover = ctx.game_state.peek().current_player();
    if !ctx.driver.controls(mover) {
        return;
    }
    let phase = *ctx.ui_state.phase().peek();
    if let Some(target) = step_focus(phase) {
        request_focus(ctx, target);
    }
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Color, Position, Rotation};

    use super::*;
    use crate::state::game_state::GameState;
    use crate::state::test_games::{seeded, tile_with_one_move, tile_with_positions};
    use crate::state::ui_state::{Hovered, TurnPhase, UiStateStoreExt};
    use crate::util::test_page::GamePage;

    fn at(x: i8, y: i8) -> Position {
        serde_json::from_value(serde_json::json!({ "x": x, "y": y })).unwrap()
    }

    fn key(name: NamedKey) -> Key {
        Key::Named(name)
    }

    #[test]
    fn keys_name_moves_unless_a_shortcut_is_held() {
        let none = Modifiers::empty();
        assert_eq!(Nav::of(&key(NamedKey::ArrowLeft), none), Some(Nav::Prev));
        assert_eq!(
            Nav::of(&key(NamedKey::ArrowDown), Modifiers::SHIFT),
            Some(Nav::Down)
        );
        assert_eq!(Nav::of(&key(NamedKey::End), none), Some(Nav::Last));
        assert_eq!(Nav::of(&key(NamedKey::ArrowLeft), Modifiers::META), None);
        assert_eq!(
            Nav::of(&key(NamedKey::ArrowRight), Modifiers::CONTROL),
            None
        );
        assert_eq!(Nav::of(&Key::Character("r".into()), none), None);

        // On the card, one row: R turns on, Shift+R back, ↑ and ↓ step too.
        let r = Key::Character("r".into());
        assert_eq!(Nav::of_rotation(&r, none), Some(Nav::Next));
        assert_eq!(
            Nav::of_rotation(&Key::Character("R".into()), Modifiers::SHIFT),
            Some(Nav::Prev)
        );
        assert_eq!(Nav::of_rotation(&r, Modifiers::CONTROL), None);
        assert_eq!(
            Nav::of_rotation(&key(NamedKey::ArrowUp), none),
            Some(Nav::Prev)
        );
        assert_eq!(
            Nav::of_rotation(&key(NamedKey::ArrowDown), none),
            Some(Nav::Next)
        );

        assert!(is_choice(&key(NamedKey::Enter), none));
        assert!(is_choice(&Key::Character(" ".into()), Modifiers::SHIFT));
        assert!(!is_choice(&key(NamedKey::Enter), Modifiers::META));
        assert!(!is_choice(&key(NamedKey::Enter), Modifiers::CONTROL));
        assert!(!is_choice(&key(NamedKey::Escape), none));
    }

    #[test]
    fn a_row_of_choices_goes_round() {
        assert_eq!(step_in(3, Some(0), Nav::Next), Some(1));
        assert_eq!(step_in(3, Some(2), Nav::Next), Some(0));
        assert_eq!(step_in(3, Some(0), Nav::Prev), Some(2));
        assert_eq!(step_in(3, Some(1), Nav::First), Some(0));
        assert_eq!(step_in(3, Some(1), Nav::Last), Some(2));
        // From nowhere, a step in starts at the near end.
        assert_eq!(step_in(3, None, Nav::Next), Some(0));
        assert_eq!(step_in(3, None, Nav::Prev), Some(2));
        assert_eq!(step_in(3, Some(1), Nav::Up), None);
        assert_eq!(step_in(0, None, Nav::First), None);
    }

    /// Spots in three rows, as on screen (y grows upwards, and a row up
    /// sits half a hex left): a and e are in one column, b and f in
    /// another, c half a hex left of b's.
    ///
    /// ```text
    ///   a(0,1)          b(2,1)
    ///                 c(1,0)            d(3,0)
    ///   e(-1,-1)        f(1,-1)
    /// ```
    fn three_rows() -> Vec<Position> {
        vec![
            at(1, -1),
            at(3, 0),
            at(0, 1),
            at(-1, -1),
            at(1, 0),
            at(2, 1),
        ]
    }

    #[test]
    fn spots_read_by_row_from_the_top_then_left_to_right() {
        let order = spot_order(three_rows());
        assert_eq!(
            order,
            [
                at(0, 1),
                at(2, 1),
                at(1, 0),
                at(3, 0),
                at(-1, -1),
                at(1, -1)
            ]
        );
    }

    #[test]
    fn left_and_right_step_through_the_spots_in_reading_order() {
        let order = spot_order(three_rows());
        let step = |from, nav| step_spot(&order, Some(from), nav);
        assert_eq!(step(at(2, 1), Nav::Next), Some(at(1, 0)));
        assert_eq!(step(at(1, 0), Nav::Prev), Some(at(2, 1)));
        // Round from the last to the first, and back.
        assert_eq!(step(at(1, -1), Nav::Next), Some(at(0, 1)));
        assert_eq!(step(at(0, 1), Nav::Prev), Some(at(1, -1)));
        assert_eq!(step(at(1, 0), Nav::First), Some(at(0, 1)));
        assert_eq!(step(at(1, 0), Nav::Last), Some(at(1, -1)));
    }

    #[test]
    fn up_and_down_go_to_the_nearest_spot_in_the_next_row_with_one() {
        let order = spot_order(three_rows());
        let step = |from, nav| step_spot(&order, Some(from), nav);
        assert_eq!(step(at(1, 0), Nav::Up), Some(at(2, 1)));
        assert_eq!(step(at(1, 0), Nav::Down), Some(at(1, -1)));
        assert_eq!(step(at(3, 0), Nav::Up), Some(at(2, 1)));
        assert_eq!(step(at(3, 0), Nav::Down), Some(at(1, -1)));
        assert_eq!(step(at(0, 1), Nav::Down), Some(at(1, 0)));
        assert_eq!(step(at(-1, -1), Nav::Up), Some(at(1, 0)));
        // No row further: the keys stay put.
        assert_eq!(step(at(0, 1), Nav::Up), Some(at(0, 1)));
        assert_eq!(step(at(-1, -1), Nav::Down), Some(at(-1, -1)));

        // A row with no spot is skipped.
        let gap = spot_order([at(0, 2), at(0, 0), at(5, 0)]);
        assert_eq!(step_spot(&gap, Some(at(0, 2)), Nav::Down), Some(at(0, 0)));
        // Two spots half a hex either side across: the left one.
        let tie = spot_order([at(1, 1), at(2, 1), at(1, 0)]);
        assert_eq!(step_spot(&tie, Some(at(1, 0)), Nav::Up), Some(at(1, 1)));
    }

    #[test]
    fn with_no_spot_active_the_keys_start_at_an_end() {
        let order = spot_order(three_rows());
        assert_eq!(step_spot(&order, None, Nav::Next), Some(at(0, 1)));
        assert_eq!(step_spot(&order, None, Nav::Down), Some(at(0, 1)));
        assert_eq!(step_spot(&order, None, Nav::Last), Some(at(1, -1)));
        // A stale spot (not among these) counts as none.
        assert_eq!(
            step_spot(&order, Some(at(9, 9)), Nav::Prev),
            Some(at(1, -1))
        );
        assert_eq!(step_spot(&[], None, Nav::Next), None);
    }

    #[test]
    fn the_hand_keys_skip_the_tiles_that_cannot_be_played() {
        let playable = [false, true, false, true, true, false];
        assert_eq!(step_tile(&playable, 1, Nav::Next), Some(3));
        assert_eq!(step_tile(&playable, 4, Nav::Next), Some(1));
        assert_eq!(step_tile(&playable, 1, Nav::Prev), Some(4));
        assert_eq!(step_tile(&playable, 3, Nav::Last), Some(4));
        assert_eq!(step_tile(&playable, 3, Nav::Up), None);

        assert_eq!(hand_focus(&playable, Some(3)), Some(3));
        assert_eq!(hand_focus(&playable, Some(2)), Some(1));
        assert_eq!(hand_focus(&playable, None), Some(1));
        assert_eq!(hand_focus(&[false, false], None), None);
    }

    #[test]
    fn only_the_tiles_that_fill_a_forced_space_are_playable_while_one_does() {
        let (game, _, _) = tile_with_positions(|_| true);
        let infos = game.hand_info();
        assert_eq!(
            playable_tiles(infos),
            infos
                .iter()
                .map(|info| info.has_moves())
                .collect::<Vec<_>>()
        );

        let forced = crate::state::test_games::fixture_after(3);
        let playable = playable_tiles(forced.hand_info());
        for (info, can) in forced.hand_info().iter().zip(playable) {
            assert_eq!(can, info.has_forced, "{info:?}");
        }
    }

    #[test]
    fn each_way_round_has_its_own_name() {
        let all = [
            Rotation::None,
            Rotation::R60,
            Rotation::R120,
            Rotation::R180,
            Rotation::R240,
            Rotation::R300,
        ];
        let names: std::collections::HashSet<_> = all.into_iter().map(rotation_label).collect();
        assert_eq!(names.len(), 6);
        assert_eq!(
            rotation_label(Rotation::R60),
            "Place it turned 60° clockwise"
        );
    }

    #[test]
    fn the_next_turns_focus_waits_for_a_move_and_a_new_turn() {
        let playable = [false, true, true];
        let next = FocusTarget::NextTurn { placed: 4 };
        assert_eq!(next.hand_tile(TurnPhase::SelectTile, 4, &playable), None);
        assert_eq!(next.hand_tile(TurnPhase::WaitingForAi, 5, &playable), None);
        assert_eq!(next.hand_tile(TurnPhase::SelectTile, 5, &playable), Some(1));

        let hand = FocusTarget::Hand(Some(2));
        assert_eq!(
            hand.hand_tile(TurnPhase::SelectPosition, 0, &playable),
            Some(2)
        );
        assert_eq!(hand.hand_tile(TurnPhase::GameOver, 0, &playable), None);
        assert_eq!(
            FocusTarget::Spots.hand_tile(TurnPhase::SelectTile, 9, &playable),
            None
        );
    }

    // --- The key handlers' actions, on a game page. ---

    fn focus_request(page: &GamePage) -> Option<FocusTarget> {
        page.with(|ctx| *ctx.ui_state.focus_request().peek())
    }

    fn phase(page: &GamePage) -> TurnPhase {
        page.with(|ctx| *ctx.ui_state.phase().peek())
    }

    fn hovered(page: &GamePage) -> Hovered {
        page.with(|ctx| *ctx.ui_state.hovered().peek())
    }

    fn spots_of(game: &GameState, idx: usize) -> Vec<Position> {
        spot_order(game.hand_info()[idx].positions.keys().copied())
    }

    #[test]
    fn enter_on_a_tile_hands_the_focus_to_the_spots() {
        let (game, idx, _) = tile_with_positions(|_| true);
        let color = game.current_player();
        let mut page = GamePage::local(game);
        page.act(|ctx| choose_tile(ctx, color, idx));
        assert_eq!(phase(&page), TurnPhase::SelectPosition);
        assert_eq!(focus_request(&page), Some(FocusTarget::Spots));

        // Enter on it again puts it back; the focus stays on it, and shows
        // where it fits again.
        page.act(|ctx| choose_tile(ctx, color, idx));
        assert_eq!(phase(&page), TurnPhase::SelectTile);
        assert_eq!(focus_request(&page), Some(FocusTarget::Hand(Some(idx))));
        assert_eq!(hovered(&page), Hovered::HandTile(idx));
    }

    #[test]
    fn a_tile_with_one_move_plays_itself_and_the_focus_waits_for_the_next_turn() {
        let (game, idx, only) = tile_with_one_move();
        let color = game.current_player();
        let before = game.board().len();
        let mut page = GamePage::local(game);
        page.act(|ctx| choose_tile(ctx, color, idx));
        assert!(page.game(|game| game.board().iter().any(|m| m.position == only.position)));
        assert_eq!(
            focus_request(&page),
            Some(FocusTarget::NextTurn { placed: before })
        );
    }

    #[test]
    fn the_spots_take_the_focus_on_the_first_and_the_arrows_move_the_ghost() {
        let (game, idx, _) = tile_with_positions(|_| true);
        let spots = spots_of(&game, idx);
        let mut page = GamePage::local(game);
        page.act(|mut ctx| ctx.ui_state.select_tile(idx));

        page.act(focus_spots);
        assert_eq!(hovered(&page), Hovered::BoardPosition(spots[0]));
        page.act(|ctx| move_spot(ctx, Nav::Next));
        assert_eq!(hovered(&page), Hovered::BoardPosition(spots[1]));
        page.act(|ctx| move_spot(ctx, Nav::Prev));
        page.act(|ctx| move_spot(ctx, Nav::Prev));
        assert_eq!(
            hovered(&page),
            Hovered::BoardPosition(*spots.last().unwrap())
        );

        // A spot already hovered (the mouse's) is where the keys go on from.
        page.act(|ctx| ctx.ui_state.hovered().set(Hovered::BoardPosition(spots[1])));
        page.act(focus_spots);
        assert_eq!(hovered(&page), Hovered::BoardPosition(spots[1]));
    }

    #[test]
    fn enter_on_a_two_way_spot_opens_the_card_with_the_focus_on_it() {
        let (game, idx, two_way) = tile_with_positions(|rotations| rotations.len() > 1);
        let mut page = GamePage::local(game);
        page.act(|mut ctx| ctx.ui_state.select_tile(idx));
        page.act(|ctx| ctx.ui_state.hovered().set(Hovered::BoardPosition(two_way)));
        page.act(choose_spot);
        assert_eq!(phase(&page), TurnPhase::SelectRotation);
        assert_eq!(
            page.with(|ctx| *ctx.ui_state.selected_position().peek()),
            Some(two_way)
        );
        assert_eq!(focus_request(&page), Some(FocusTarget::Card(0)));

        // Escape: back to the spot, which the keys stay on…
        page.act(step_back);
        assert_eq!(phase(&page), TurnPhase::SelectPosition);
        assert_eq!(hovered(&page), Hovered::BoardPosition(two_way));
        assert_eq!(focus_request(&page), Some(FocusTarget::Spots));
        // …and again: back to the tile.
        page.act(step_back);
        assert_eq!(phase(&page), TurnPhase::SelectTile);
        assert_eq!(focus_request(&page), Some(FocusTarget::Hand(Some(idx))));
    }

    #[test]
    fn tabbing_back_from_the_card_the_keys_stay_on_its_spot() {
        let (game, idx, two_way) = tile_with_positions(|rotations| rotations.len() > 1);
        let mut page = GamePage::local(game);
        page.act(|mut ctx| ctx.ui_state.select_tile(idx));
        page.act(|ctx| click_position(ctx, two_way));
        // Leaving the card cleared its option's hover; the spots take the
        // focus on the spot it is open at, not on the first.
        page.act(|ctx| ctx.ui_state.hovered().set(Hovered::None));
        page.act(focus_spots);
        assert_eq!(hovered(&page), Hovered::BoardPosition(two_way));
    }

    #[test]
    fn enter_on_an_option_plays_it_and_the_next_human_takes_the_focus() {
        let (game, idx, two_way) = tile_with_positions(|rotations| rotations.len() > 1);
        let mover = game.current_player();
        let before = game.board().len();
        let second = game.hand_info()[idx].positions[&two_way][1];
        let mut page = GamePage::local(game);
        page.act(|mut ctx| ctx.ui_state.select_tile(idx));
        page.act(|ctx| click_position(ctx, two_way));
        page.act(|ctx| choose_rotation(ctx, second));

        let placed = page.game(|game| game.board().iter().find(|m| m.position == two_way).copied());
        assert_eq!(placed.map(|m| m.rotation), Some(second));
        let request = focus_request(&page).unwrap();
        assert_eq!(request, FocusTarget::NextTurn { placed: before });
        // Both seats are played here: the next one's hand takes it at once.
        let (next, playable) =
            page.game(|game| (game.current_player(), playable_tiles(game.hand_info())));
        assert_ne!(next, mover);
        assert_eq!(
            request.hand_tile(phase(&page), before + 1, &playable),
            hand_focus(&playable, None)
        );
    }

    #[test]
    fn enter_on_a_one_way_spot_plays_it() {
        let (game, idx, one_way) = tile_with_positions(|rotations| rotations.len() == 1);
        let before = game.board().len();
        let mut page = GamePage::local(game);
        page.act(|mut ctx| ctx.ui_state.select_tile(idx));
        page.act(|ctx| ctx.ui_state.hovered().set(Hovered::BoardPosition(one_way)));
        page.act(choose_spot);
        assert_eq!(page.game(|game| game.board().len()), before + 1);
        assert_eq!(
            focus_request(&page),
            Some(FocusTarget::NextTurn { placed: before })
        );
    }

    #[test]
    fn enter_with_no_spot_active_does_nothing() {
        let (game, idx, _) = tile_with_positions(|_| true);
        let mut page = GamePage::local(game);
        page.act(|mut ctx| ctx.ui_state.select_tile(idx));
        page.act(|ctx| ctx.ui_state.focus_request().set(None));
        page.act(choose_spot);
        assert_eq!(phase(&page), TurnPhase::SelectPosition);
        assert_eq!(focus_request(&page), None);
    }

    #[test]
    fn the_hand_arrows_skip_to_the_next_playable_tile() {
        let game = crate::state::test_games::fixture_after(3);
        let playable = playable_tiles(game.hand_info());
        let forced = playable.iter().position(|&can| can).unwrap();
        let mut page = GamePage::local(game);
        // One tile fills the forced space: the arrows have nowhere else to go.
        page.act(|ctx| step_hand(ctx, forced, Nav::Next));
        assert_eq!(focus_request(&page), Some(FocusTarget::Hand(Some(forced))));

        let mut page = GamePage::local(seeded(3));
        page.act(|ctx| step_hand(ctx, 5, Nav::Next));
        assert_eq!(focus_request(&page), Some(FocusTarget::Hand(Some(0))));
    }

    #[test]
    fn a_key_on_the_bare_screen_hands_the_focus_to_the_turns_step() {
        let mut page = GamePage::local(seeded(3));
        page.act(enter_turn);
        assert_eq!(focus_request(&page), Some(FocusTarget::Hand(None)));
        page.act(|mut ctx| ctx.ui_state.select_tile(0));
        page.act(enter_turn);
        assert_eq!(focus_request(&page), Some(FocusTarget::Spots));
    }

    #[test]
    fn online_the_next_turns_focus_waits_for_the_rooms_snapshot() {
        let (game, idx, one_way) = tile_with_positions(|rotations| rotations.len() == 1);
        let mover = game.current_player();
        let before = game.board().len();
        let (mut page, _commands) =
            GamePage::online(game, Some(mover), tantrix_game::EnumMap::default());
        page.act(|mut ctx| ctx.ui_state.select_tile(idx));
        page.act(|ctx| ctx.ui_state.hovered().set(Hovered::BoardPosition(one_way)));
        page.act(choose_spot);
        // Sent to the room; until its snapshot the board is as it was, and
        // the hand that still shows the tile does not take the focus.
        assert_eq!(page.game(|game| game.board().len()), before);
        let request = focus_request(&page).unwrap();
        assert_eq!(request, FocusTarget::NextTurn { placed: before });
        let playable = page.game(|game| playable_tiles(game.hand_info()));
        assert_eq!(request.hand_tile(phase(&page), before, &playable), None);
    }

    #[test]
    fn a_watcher_or_a_waiting_player_gets_no_focus_from_the_bare_screen() {
        let game = seeded(3);
        let mover = game.current_player();
        let other = [Color::Red, Color::Blue]
            .into_iter()
            .find(|&c| c != mover)
            .unwrap();
        let (mut page, _commands) =
            GamePage::online(game, Some(other), tantrix_game::EnumMap::default());
        page.act(enter_turn);
        assert_eq!(focus_request(&page), None);
    }
}
