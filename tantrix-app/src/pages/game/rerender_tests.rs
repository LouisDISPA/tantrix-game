//! Pins which components a state change re-renders on the game screen.
//!
//! Each test mounts the real `GamePlay` screen in a `VirtualDom` (a local
//! game page built with `use_game_ctx`), makes one change the way an event
//! handler would, lets every memo, effect and dirty component catch up, and
//! reads the render counts the components record under `cfg(test)`
//! (`util::render_count`). A count is deterministic: nothing here waits on
//! time.

use dioxus::prelude::*;
use tantrix_game::{Color, Move, Position};

use super::GamePlay;
use crate::hooks::camera::BoardCamera;
use crate::hooks::use_game_ctx;
use crate::state::actions::{click_position, execute_move, start_game};
use crate::state::config::{Difficulty, GameConfig, PlayerKind};
use crate::state::game_driver::GameDriver;
use crate::state::game_state::{GameCtx, GameState};
use crate::state::gesture::TOUCH_SLOP;
use crate::state::ui_state::{Hovered, TurnPhase, UiStateStoreExt, UiStateStoreImplExt};
use crate::util::render_count;
use crate::util::test_page::settle;

/// Recorded by a memo of the test page each time it recomputes: it reads
/// every `UiState` field but the camera's (`zoom`, `pan_x`, `pan_y`,
/// `gesture`, `dragged`, `stage`), so it counts the writes that reach any
/// of them.
const NON_CAMERA_FIELDS: &str = "non-camera UiState fields";

/// The components of the board and hands that record their renders.
const BOARD_AND_HANDS: [&str; 9] = [
    "GamePlay",
    "GameHud",
    "BoardViewport",
    "BoardTilesLayer",
    "BoardEmptyLayer",
    "BoardPreview",
    "OrientationPicker",
    "ZoomControls",
    "PlayerHand",
];

/// A local game page showing `game` on the game screen.
struct Page {
    dom: VirtualDom,
}

impl Page {
    fn new(game: GameState) -> Self {
        let mut dom = VirtualDom::new_with_props(
            |game: GameState| {
                let ctx = use_game_ctx(GameDriver::Local, move || game);
                let ui = ctx.ui_state;
                use_memo(move || {
                    render_count::record(NON_CAMERA_FIELDS, ());
                    (
                        ui.phase()(),
                        ui.message()(),
                        ui.selected_tile()(),
                        ui.selected_position()(),
                        ui.hovered()(),
                        ui.peeked()(),
                        ui.new_tile()(),
                        ui.last_placed_position()(),
                        ui.highlighted_color()(),
                    )
                });
                rsx! { GamePlay {} }
            },
            game,
        );
        dom.rebuild_in_place();
        settle(&mut dom);
        Self { dom }
    }

    fn ctx(&self) -> GameCtx {
        self.dom.in_scope(ScopeId::APP, consume_context::<GameCtx>)
    }

    /// Make `change` as an event handler would (outside any render), then
    /// let the page catch up.
    fn act(&mut self, change: impl FnOnce(GameCtx)) {
        let ctx = self.ctx();
        self.dom.in_scope(ScopeId::APP, || change(ctx));
        settle(&mut self.dom);
    }

    fn phase(&self) -> TurnPhase {
        *self.ctx().ui_state.phase().peek()
    }

    fn game<T>(&self, read: impl FnOnce(&GameState) -> T) -> T {
        let ctx = self.ctx();
        self.dom
            .in_scope(ScopeId::APP, || read(&ctx.game_state.peek()))
    }
}

/// Assert that none of `components` rendered since the last reset.
#[track_caller]
fn assert_quiet(components: &[&str]) {
    for &component in components {
        assert_eq!(render_count::total(component), 0, "{component} re-rendered");
    }
}

/// Every component of the board and hands, but `except`.
fn all_but(except: &[&str]) -> Vec<&'static str> {
    BOARD_AND_HANDS
        .into_iter()
        .filter(|name| !except.contains(name))
        .collect()
}

fn config(players: &[(Color, PlayerKind)], seed: u64) -> GameConfig {
    let mut config = GameConfig::new(Some(seed));
    for (color, kind) in config.players.iter_mut() {
        *kind = players
            .iter()
            .find(|(c, _)| c == color)
            .map_or(PlayerKind::Off, |&(_, k)| k);
    }
    config
}

fn humans(colors: &[Color], seed: u64) -> GameConfig {
    let players: Vec<_> = colors.iter().map(|&c| (c, PlayerKind::Human)).collect();
    config(&players, seed)
}

/// A seeded two-player game one move in (a tile on an empty board plays
/// itself), whose player to move holds a tile — at `idx` — with several
/// positions, one of which (`at`) takes several rotations.
fn game_with_choices() -> (GameState, usize, Position) {
    (0..200)
        .find_map(|seed| {
            let mut game = GameState::new(&humans(&[Color::Red, Color::Blue], seed));
            let first = *game.legal_moves().first()?;
            game.play_move(first.placement()).ok()?;
            let (idx, at) = game
                .hand_info()
                .iter()
                .enumerate()
                .find_map(|(idx, info)| {
                    if info.positions.len() < 2 {
                        return None;
                    }
                    let (&at, _) = info.positions.iter().find(|(_, rots)| rots.len() > 1)?;
                    Some((idx, at))
                })?;
            Some((game, idx, at))
        })
        .expect("a seed leaves a tile with several positions and rotations")
}

/// Play the first legal move for the player to move, through the same
/// action a click plays it with.
fn play_first_legal_move(page: &mut Page) -> Move {
    let (m, idx) = page.game(|game| {
        let m = game.legal_moves()[0];
        let player = game.current_player();
        let idx = game.hand(player).iter().position(|&t| t == m.tile).unwrap();
        (m, idx)
    });
    page.act(|ctx| execute_move(ctx, idx, m.position, m.rotation));
    m
}

#[test]
fn hovering_a_hex_re_renders_only_the_preview() {
    let (game, idx, at) = game_with_choices();
    let mut page = Page::new(game);
    page.act(|mut ctx| ctx.ui_state.select_tile(idx));
    assert_eq!(page.phase(), TurnPhase::SelectPosition);

    render_count::reset();
    page.act(|ctx| ctx.ui_state.hovered().set(Hovered::BoardPosition(at)));
    // The ghost tile appears at the hovered hex…
    assert_eq!(render_count::total("BoardPreview"), 1);
    assert_eq!(render_count::of("BoardTile", at), 1);
    // …and nothing else re-renders: not the hexes, not the card of options.
    assert_quiet(&all_but(&["BoardPreview"]));

    page.act(|ctx| ctx.ui_state.hovered().set(Hovered::None));
    assert_eq!(render_count::total("BoardPreview"), 2);
    assert_quiet(&all_but(&["BoardPreview"]));
}

#[test]
fn hovering_a_rotation_re_renders_only_the_preview() {
    let (game, idx, at) = game_with_choices();
    let mut page = Page::new(game);
    page.act(|mut ctx| ctx.ui_state.select_tile(idx));
    render_count::reset();
    page.act(|ctx| click_position(ctx, at));
    assert_eq!(page.phase(), TurnPhase::SelectRotation);
    // The card of options opened.
    assert_eq!(render_count::total("OrientationPicker"), 1);

    let rotations = page.game(|game| game.hand_info()[idx].positions[&at].clone());
    for &rotation in &rotations {
        render_count::reset();
        page.act(|ctx| ctx.ui_state.hovered().set(Hovered::Rotation(rotation)));
        page.act(|ctx| ctx.ui_state.hovered().set(Hovered::None));
        // The ghost shows the option pointed at, and goes when it is let go;
        // the card and the rest of the board stay as they are.
        assert!(render_count::total("BoardPreview") <= 2);
        assert_quiet(&all_but(&["BoardPreview"]));
    }
    assert!(rotations.len() > 1);
}

#[test]
fn dragging_the_board_re_renders_only_the_viewport() {
    let mut page = Page::new(game_with_choices().0);
    let placed = page.game(|game| game.board().iter().map(|m| m.position).collect::<Vec<_>>());

    render_count::reset();
    page.act(|ctx| BoardCamera::new(ctx.ui_state).pointer_down(1, 100.0, 100.0, TOUCH_SLOP));
    for step in 1..=3 {
        let x = 100.0 + 10.0 * f64::from(step);
        page.act(move |ctx| BoardCamera::new(ctx.ui_state).pointer_move(1, x, 90.0));
    }
    page.act(|ctx| BoardCamera::new(ctx.ui_state).pointer_up(1));

    assert_eq!(*page.ctx().ui_state.pan_x().peek(), -30.0);
    assert_eq!(*page.ctx().ui_state.pan_y().peek(), 10.0);
    // Once per pan step (the first, past the slop of a tap, also adds the
    // drag's class) and once as the drag ends.
    assert_eq!(render_count::total("BoardViewport"), 4);
    assert_quiet(&all_but(&["BoardViewport"]));
    for at in placed {
        assert_eq!(render_count::of("BoardTile", at), 0);
    }
    // Only camera fields were written: no other UiState reader woke.
    assert_eq!(render_count::total(NON_CAMERA_FIELDS), 0);
}

#[test]
fn zooming_re_renders_only_the_viewport_and_the_zoom_buttons() {
    let mut page = Page::new(game_with_choices().0);

    render_count::reset();
    page.act(|ctx| BoardCamera::new(ctx.ui_state).zoom_in());
    page.act(|ctx| BoardCamera::new(ctx.ui_state).on_wheel(-1.0));
    page.act(|ctx| BoardCamera::new(ctx.ui_state).reset_zoom());
    assert_eq!(render_count::total("ZoomControls"), 3);
    assert_eq!(render_count::total("BoardViewport"), 3);
    assert_quiet(&all_but(&["ZoomControls", "BoardViewport"]));
    assert_eq!(render_count::total(NON_CAMERA_FIELDS), 0);

    // Resetting an unmoved camera writes nothing at all.
    page.act(|ctx| BoardCamera::new(ctx.ui_state).reset_zoom());
    assert_eq!(render_count::total("ZoomControls"), 3);
    assert_eq!(render_count::total("BoardViewport"), 3);
}

#[test]
fn a_move_re_renders_only_the_hands_it_changes() {
    let three = [Color::Red, Color::Blue, Color::Yellow];
    let mut page = Page::new(GameState::new(&humans(&three, 3)));
    let mover = page.game(|game| game.current_player());
    let scores_before = page.game(|game| game.scores());

    render_count::reset();
    play_first_legal_move(&mut page);
    let next = page.game(|game| game.current_player());
    let scores_after = page.game(|game| game.scores());
    assert_ne!(next, mover);

    // The mover drew a tile, and the turn passed from them to `next`…
    assert_eq!(render_count::of("PlayerHand", mover), 1);
    assert_eq!(render_count::of("PlayerHand", next), 1);
    // …while the third hand is just as it was, unless the tile put down
    // lengthened (or cut) its line: its header shows the score.
    for bystander in three.into_iter().filter(|&c| c != mover && c != next) {
        let rescored = scores_before[bystander].score != scores_after[bystander].score;
        assert_eq!(
            render_count::of("PlayerHand", bystander),
            usize::from(rescored),
            "{bystander}"
        );
    }
}

#[test]
fn peeking_at_a_seat_re_renders_only_the_two_hands_whose_tab_flips_and_the_hud() {
    let three = [Color::Red, Color::Blue, Color::Yellow];
    let mut page = Page::new(GameState::new(&humans(&three, 3)));
    let mover = page.game(|game| game.current_player());
    let peeked = three.into_iter().find(|&c| c != mover).unwrap();
    let third = three
        .into_iter()
        .find(|&c| c != mover && c != peeked)
        .unwrap();

    render_count::reset();
    page.act(|ctx| ctx.ui_state.peeked().set(Some(peeked)));
    assert_eq!(render_count::of("PlayerHand", mover), 1);
    assert_eq!(render_count::of("PlayerHand", peeked), 1);
    assert_eq!(render_count::of("PlayerHand", third), 0);
    // The HUD says whose tiles are looked at (on a wide window); no tab, tile
    // or board layer beyond those two hands and it wakes.
    assert_eq!(render_count::total("GameHud"), 1);
    assert_quiet(&all_but(&["PlayerHand", "GameHud"]));

    // The peek lasts for one turn.
    play_first_legal_move(&mut page);
    assert_eq!(*page.ctx().ui_state.peeked().peek(), None);
}

#[test]
fn a_move_re_renders_only_the_tiles_it_changes() {
    let mut page = Page::new(GameState::new(&humans(&[Color::Red, Color::Blue], 3)));
    let first = play_first_legal_move(&mut page);
    let second = play_first_legal_move(&mut page);

    render_count::reset();
    let third = play_first_legal_move(&mut page);
    // The board lists tiles by position, and the new one comes first: a
    // list diffed by index instead of by key would remount the others.
    assert!(third.position < second.position && second.position < first.position);

    // The new tile appears just placed; the previous one loses its flag;
    // the first stays as it was (mounted, not re-rendered).
    assert_eq!(render_count::of("BoardTile", third.position), 1);
    assert_eq!(render_count::of("BoardTile", second.position), 1);
    assert_eq!(render_count::of("BoardTile", first.position), 0);
    assert_eq!(render_count::total("BoardTilesLayer"), 1);
}

#[test]
fn showing_the_game_screen_changes_no_state() {
    // An AI is to move, but only Start Game hands it the turn: mounting the
    // screen (and every re-render after) writes nothing.
    let ais = config(
        &[
            (Color::Red, PlayerKind::Ai(Difficulty::Easy)),
            (Color::Blue, PlayerKind::Ai(Difficulty::Easy)),
        ],
        3,
    );
    let page = Page::new(GameState::new(&ais));
    assert_eq!(page.phase(), TurnPhase::SelectTile);
    assert!(page.game(|game| game.history().is_empty()));
    assert_eq!(render_count::of("GamePlay", ()), 1);
}

#[test]
fn starting_a_game_hands_the_first_turn_on() {
    let page = Page::new(GameState::new(&humans(&[Color::Red, Color::Blue], 3)));
    let ctx = page.ctx();

    // Whoever starts in an all-AI game is an AI: the AI loop takes the turn.
    let ais = config(
        &[
            (Color::Red, PlayerKind::Ai(Difficulty::Easy)),
            (Color::Blue, PlayerKind::Ai(Difficulty::Easy)),
        ],
        5,
    );
    page.dom.in_scope(ScopeId::APP, || start_game(ctx, &ais));
    assert_eq!(page.phase(), TurnPhase::WaitingForAi);

    // A human starts by choosing a tile.
    let people = humans(&[Color::Red, Color::Green], 5);
    page.dom.in_scope(ScopeId::APP, || start_game(ctx, &people));
    assert_eq!(page.phase(), TurnPhase::SelectTile);
    let mut colors = page.game(|game| game.playing_colors().to_vec());
    colors.sort_by_key(|&c| c as u8);
    assert_eq!(colors, [Color::Red, Color::Green]);
}
