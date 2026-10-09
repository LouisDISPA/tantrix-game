use dioxus::prelude::*;
use tantrix_game::Position;

use crate::state::game_state::GameCtx;
use crate::state::gesture::{Pointer, Step};
use crate::state::ui_state::{set_if_changed, UiState, UiStateStoreExt};
use crate::svg::camera_math::{self, home_zoom, pan_by, pinch, Pose, Size, MAX_ZOOM, MIN_ZOOM};
use crate::svg::card_placement;
use crate::svg::geometry::{board_center, hex_rect, hexes_rect, Rect, HEX_SIZE};

/// The board camera's controls: zoom in/out, reset, wheel zoom, and the
/// pointers (drag to pan, two fingers to pinch). They write the camera
/// fields of `UiState` one at a time (`zoom`, `pan_x`, `pan_y`, `gesture`,
/// `dragged`), so a drag's pointer moves wake the readers of the pan only,
/// never the rest of the board.
///
/// Holds nothing but the `Copy`able `Store`, so each method copies it out of
/// `self` — what event-handler closures in RSX need. It reads nothing either:
/// each component reads the camera fields it shows (the viewport its
/// transform, the zoom buttons the zoom level).
#[derive(Clone, Copy)]
pub struct BoardCamera {
    ui_state: Store<UiState>,
}

impl BoardCamera {
    pub fn new(ui_state: Store<UiState>) -> Self {
        Self { ui_state }
    }

    /// Where the camera looks now.
    pub fn pose(self) -> Pose {
        let ui = self.ui_state;
        Pose {
            zoom: *ui.zoom().peek(),
            pan_x: *ui.pan_x().peek(),
            pan_y: *ui.pan_y().peek(),
        }
    }

    /// Look from `pose`, writing only what differs from where the camera is.
    pub fn set_pose(self, pose: Pose) {
        let ui = self.ui_state;
        set_if_changed(ui.zoom(), pose.zoom);
        set_if_changed(ui.pan_x(), pose.pan_x);
        set_if_changed(ui.pan_y(), pose.pan_y);
    }

    pub fn zoom_in(self) {
        self.zoom_with(|zoom| zoom * 1.25);
    }

    pub fn zoom_out(self) {
        self.zoom_with(|zoom| zoom / 1.25);
    }

    /// The stage's box on the screen has been measured: remember it. A camera
    /// still at the home zoom of the stage it had stays at the home zoom of
    /// the new one, so a board starts (and stays, as the window is resized)
    /// as large as the window suits until the player zooms by hand.
    pub fn stage_measured(self, stage: Rect) {
        let ui = self.ui_state;
        let at_home = ui
            .stage()
            .peek()
            .is_none_or(|before| *ui.zoom().peek() == home_zoom(Size::of(&before)));
        set_if_changed(ui.stage(), Some(stage));
        if at_home {
            set_if_changed(ui.zoom(), home_zoom(Size::of(&stage)));
        }
    }

    /// The zoom a board starts at on the stage as measured: 100% until it
    /// is.
    fn stage_home_zoom(self) -> f64 {
        self.ui_state
            .stage()
            .peek()
            .map_or(1.0, |stage| home_zoom(Size::of(&stage)))
    }

    /// Back to the home zoom (100% on a phone or a laptop), centered.
    pub fn reset_zoom(self) {
        self.set_pose(Pose::HOME.with_zoom(self.stage_home_zoom()));
    }

    /// Zoom in a step for a wheel scrolled up (`delta_y < 0`), out otherwise.
    pub fn on_wheel(self, delta_y: f64) {
        let factor = if delta_y < 0.0 { 1.1 } else { 0.9 };
        self.zoom_with(|zoom| zoom * factor);
    }

    /// A pointer went down at (`x`, `y`) on the screen: a pan starts, or
    /// with a second pointer a pinch. `slop` is how far it may wobble and
    /// still be a tap (`gesture::TOUCH_SLOP` / `MOUSE_SLOP`).
    pub fn pointer_down(self, id: i32, x: f64, y: f64, slop: f64) {
        let ui = self.ui_state;
        set_if_changed(ui.dragged(), false);
        let gesture = (*ui.gesture().peek()).down(Pointer::new(id, x, y), slop);
        set_if_changed(ui.gesture(), gesture);
    }

    /// A pointer moved to (`x`, `y`): the board follows it once it has gone
    /// past the slop of a tap, or is pinched between two.
    pub fn pointer_move(self, id: i32, x: f64, y: f64) {
        let ui = self.ui_state;
        let (gesture, step) = (*ui.gesture().peek()).moved(Pointer::new(id, x, y));
        // Its readers only look at whether the board is being dragged,
        // through a memo.
        set_if_changed(ui.gesture(), gesture);
        match step {
            Step::None => {}
            Step::Pan { dx, dy } => self.set_pose(pan_by(self.pose(), dx, dy)),
            Step::Pinch { factor, from, to } => {
                let mut pose = self.pose();
                // About the fingers' midpoint, which needs the stage's place on
                // the screen; until it is measured the board only follows them.
                if let Some(stage) = *ui.stage().peek() {
                    let pivot = (from.0 - stage.min_x, from.1 - stage.min_y);
                    pose = pinch(pose, pivot, Size::of(&stage), factor, MIN_ZOOM, MAX_ZOOM);
                }
                self.set_pose(pan_by(pose, to.0 - from.0, to.1 - from.1));
            }
        }
        if gesture.is_drag() {
            set_if_changed(ui.dragged(), true);
        }
    }

    /// A pointer lifted or was cancelled.
    pub fn pointer_up(self, id: i32) {
        let ui = self.ui_state;
        let gesture = (*ui.gesture().peek()).up(id);
        set_if_changed(ui.gesture(), gesture);
    }

    /// The mouse left the board: whatever it was doing there ends.
    pub fn pointer_leave(self) {
        set_if_changed(self.ui_state.gesture(), Default::default());
    }

    /// Whether the last gesture moved the board — then the click that comes
    /// with its lift is not a tap on what is under the finger.
    pub fn was_drag(self) -> bool {
        *self.ui_state.dragged().peek()
    }

    /// The whole board on the stage, as large as it can be (at most
    /// [`camera_math::max_fit_zoom`]). Does nothing until the stage has been
    /// measured, or on an empty board.
    pub fn fit_board(self, placed: &[Position]) {
        let Some(stage) = *self.ui_state.stage().peek() else {
            return;
        };
        let Some(rect) = hexes_rect(placed, HEX_SIZE) else {
            return;
        };
        self.set_pose(camera_math::fit(
            rect,
            board_center(placed, HEX_SIZE),
            Size::of(&stage),
            camera_math::FIT_PAD,
            MIN_ZOOM,
            camera_math::max_fit_zoom(Size::of(&stage)),
        ));
    }

    /// After a move: the board back in the middle at the player's zoom,
    /// zoomed out if it no longer fits. `placed` is the board's tiles. Does
    /// nothing until the stage has been measured.
    pub fn frame_board(self, placed: &[Position]) {
        let Some(stage) = *self.ui_state.stage().peek() else {
            return;
        };
        let Some(rect) = hexes_rect(placed, HEX_SIZE) else {
            return;
        };
        let center = board_center(placed, HEX_SIZE);
        let zoom = *self.ui_state.zoom().peek();
        self.set_pose(camera_math::after_move(
            rect,
            zoom,
            center,
            Size::of(&stage),
        ));
    }

    /// After a tile is selected: the camera as it was while every place the
    /// tile fits (`spots`) is on the stage and wide enough to tap, else
    /// framing them with the tiles. Does nothing until the stage has been
    /// measured.
    pub fn frame_spots(self, placed: &[Position], spots: &[Position]) {
        let Some(stage) = *self.ui_state.stage().peek() else {
            return;
        };
        let Some(spots) = hexes_rect(spots, HEX_SIZE) else {
            return;
        };
        let center = board_center(placed, HEX_SIZE);
        self.set_pose(camera_math::on_select(
            hexes_rect(placed, HEX_SIZE),
            spots,
            self.pose(),
            center,
            Size::of(&stage),
        ));
    }

    /// After a two-way spot is picked: the camera as it was while the card
    /// of options has room beside the spot `at`, else the spot centred.
    /// `placed` is the board's tiles. Does nothing until the stage has been
    /// measured.
    pub fn make_room_for_card(self, placed: &[Position], at: Position) {
        let Some(stage) = *self.ui_state.stage().peek() else {
            return;
        };
        self.set_pose(card_placement::make_room(
            hex_rect(at, HEX_SIZE),
            self.pose(),
            board_center(placed, HEX_SIZE),
            Size::of(&stage),
        ));
    }

    fn zoom_with(self, change: impl FnOnce(f64) -> f64) {
        let zoom = *self.ui_state.zoom().peek();
        set_if_changed(self.ui_state.zoom(), change(zoom).clamp(MIN_ZOOM, MAX_ZOOM));
    }
}

/// The board camera of the game on screen (see [`BoardCamera`]). Subscribes
/// to nothing.
pub fn use_board_camera() -> BoardCamera {
    BoardCamera::new(use_context::<GameCtx>().ui_state)
}

/// The viewport's custom properties for a camera pose. They feed the
/// `transform` declared in CSS, which keeps transitions and easing
/// configurable without touching Rust.
pub fn camera_style(zoom: f64, pan_x: f64, pan_y: f64) -> String {
    format!("--zoom: {zoom:.3}; --pan-x: {pan_x:.1}px; --pan-y: {pan_y:.1}px;")
}

/// The zoom level as the reset button shows it.
pub fn zoom_percent(zoom: f64) -> i32 {
    (zoom * 100.0) as i32
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;
    use tantrix_game::Position;

    use super::{camera_style, home_zoom, zoom_percent, BoardCamera, MAX_ZOOM, MIN_ZOOM};
    use crate::state::game_state::GameState;
    use crate::state::gesture::TOUCH_SLOP;
    use crate::state::test_games::{seeded, tile_with_positions};
    use crate::state::ui_state::UiStateStoreExt;
    use crate::svg::camera_math::{
        rect_to_stage, shows, spot_width, to_board, Pose, Size, FIT_PAD, MIN_SPOT_ZOOM, MIN_TARGET,
    };
    use crate::svg::geometry::{board_center, hexes_rect, Rect, HEX_SIZE};
    use crate::util::test_page::GamePage;

    /// The camera's (zoom, pan_x, pan_y) after `moves`.
    fn camera_after(moves: impl FnOnce(BoardCamera)) -> (f64, f64, f64) {
        let mut page = GamePage::local(seeded(3));
        page.act(|ctx| moves(BoardCamera::new(ctx.ui_state)));
        page.with(|ctx| {
            let ui = ctx.ui_state;
            (*ui.zoom().peek(), *ui.pan_x().peek(), *ui.pan_y().peek())
        })
    }

    #[test]
    fn the_viewport_style_carries_the_camera_pose() {
        assert_eq!(
            camera_style(1.0, 0.0, 0.0),
            "--zoom: 1.000; --pan-x: 0.0px; --pan-y: 0.0px;"
        );
        assert_eq!(
            camera_style(1.25, -12.345, 7.06),
            "--zoom: 1.250; --pan-x: -12.3px; --pan-y: 7.1px;"
        );
    }

    #[test]
    fn the_zoom_shows_in_whole_percent() {
        assert_eq!(zoom_percent(1.0), 100);
        assert_eq!(zoom_percent(1.25), 125);
        assert_eq!(zoom_percent(MIN_ZOOM), 30);
        assert_eq!(zoom_percent(MAX_ZOOM), 500);
        // Cut, not rounded.
        assert_eq!(zoom_percent(1.1 * 1.1), 121);
        assert_eq!(zoom_percent(0.999), 99);
    }

    #[test]
    fn zooming_stops_at_its_limits() {
        let (zoom, ..) = camera_after(|camera| (0..20).for_each(|_| camera.zoom_in()));
        assert_eq!(zoom, MAX_ZOOM);
        let (zoom, ..) = camera_after(|camera| (0..20).for_each(|_| camera.zoom_out()));
        assert_eq!(zoom, MIN_ZOOM);
        let (zoom, ..) = camera_after(|camera| (0..40).for_each(|_| camera.on_wheel(1.0)));
        assert_eq!(zoom, MIN_ZOOM);
    }

    #[test]
    fn the_wheel_zooms_in_when_scrolled_up() {
        let (up, ..) = camera_after(|camera| camera.on_wheel(-3.0));
        let (down, ..) = camera_after(|camera| camera.on_wheel(3.0));
        assert!((up - 1.1).abs() < 1e-9);
        assert!((down - 0.9).abs() < 1e-9);
    }

    #[test]
    fn a_drag_pans_by_screen_distance_over_zoom() {
        let pose = camera_after(|camera| {
            camera.zoom_in();
            camera.zoom_in(); // 1.5625
            camera.pointer_down(1, 10.0, 10.0, TOUCH_SLOP);
            camera.pointer_move(1, 41.25, -5.625);
            camera.pointer_up(1);
            // After the drag ends, moving the pointer pans nothing.
            camera.pointer_move(1, 500.0, 500.0);
        });
        assert_eq!(pose, (1.5625, -20.0, 10.0));
    }

    #[test]
    fn a_tap_pans_nothing_and_is_no_drag() {
        let (pose, dragged) = {
            let mut page = GamePage::local(seeded(3));
            page.act(|ctx| {
                let camera = BoardCamera::new(ctx.ui_state);
                camera.pointer_down(1, 100.0, 100.0, TOUCH_SLOP);
                camera.pointer_move(1, 103.0, 98.0);
                camera.pointer_up(1);
            });
            page.with(|ctx| {
                let camera = BoardCamera::new(ctx.ui_state);
                (camera.pose(), camera.was_drag())
            })
        };
        assert_eq!(pose, Pose::HOME);
        assert!(!dragged);
    }

    #[test]
    fn the_click_after_a_drag_is_marked_until_the_next_press() {
        let mut page = GamePage::local(seeded(3));
        let dragged = |page: &GamePage| page.with(|ctx| BoardCamera::new(ctx.ui_state).was_drag());
        page.act(|ctx| {
            let camera = BoardCamera::new(ctx.ui_state);
            camera.pointer_down(1, 0.0, 0.0, TOUCH_SLOP);
            camera.pointer_move(1, 40.0, 0.0);
        });
        assert!(dragged(&page));
        // Lifted: the click that follows still sees it.
        page.act(|ctx| BoardCamera::new(ctx.ui_state).pointer_up(1));
        assert!(dragged(&page));
        page.act(|ctx| BoardCamera::new(ctx.ui_state).pointer_down(1, 0.0, 0.0, TOUCH_SLOP));
        assert!(!dragged(&page));
    }

    #[test]
    fn a_mouse_leaving_the_board_ends_the_drag() {
        let pose = camera_after(|camera| {
            camera.pointer_down(1, 0.0, 0.0, TOUCH_SLOP);
            camera.pointer_move(1, 20.0, 0.0);
            camera.pointer_leave();
            camera.pointer_move(1, 80.0, 0.0);
        });
        assert_eq!(pose, (1.0, -20.0, 0.0));
    }

    #[test]
    fn two_fingers_pinch_the_board_about_their_midpoint() {
        let stage = Rect::new(10.0, 50.0, 400.0, 650.0);
        let mut page = GamePage::local(seeded(3));
        page.act(|ctx| ctx.ui_state.stage().set(Some(stage)));
        let size = Size::of(&stage);
        // The board point under the fingers' midpoint (stage-relative (195, 100)).
        let under_fingers = |page: &GamePage| {
            page.with(|ctx| {
                to_board(
                    (195.0, 100.0),
                    BoardCamera::new(ctx.ui_state).pose(),
                    (0.0, 0.0),
                    size,
                )
            })
        };
        let before = under_fingers(&page);

        page.act(|ctx| {
            let camera = BoardCamera::new(ctx.ui_state);
            camera.pointer_down(1, 10.0 + 145.0, 50.0 + 100.0, TOUCH_SLOP);
            camera.pointer_down(2, 10.0 + 245.0, 50.0 + 100.0, TOUCH_SLOP);
            // The fingers spread to twice the distance, midpoint kept.
            camera.pointer_move(1, 10.0 + 95.0, 50.0 + 100.0);
            camera.pointer_move(2, 10.0 + 295.0, 50.0 + 100.0);
        });
        let pose = page.with(|ctx| BoardCamera::new(ctx.ui_state).pose());
        assert!(pose.zoom > 1.9, "{pose:?}");
        let after = under_fingers(&page);
        assert!(
            (before.0 - after.0).abs() < 1e-6 && (before.1 - after.1).abs() < 1e-6,
            "{before:?} vs {after:?}"
        );
        assert!(page.with(|ctx| BoardCamera::new(ctx.ui_state).was_drag()));
    }

    #[test]
    fn resetting_the_zoom_recentres_the_board() {
        let pose = camera_after(|camera| {
            camera.zoom_out();
            camera.pointer_down(1, 0.0, 0.0, TOUCH_SLOP);
            camera.pointer_move(1, 30.0, 40.0);
            camera.pointer_up(1);
            camera.reset_zoom();
        });
        assert_eq!(pose, (1.0, 0.0, 0.0));
    }

    // ---- the zoom a board starts at

    #[test]
    fn a_roomy_stage_starts_zoomed_in_and_a_phones_at_100_percent() {
        let zoom_on = |stage: Rect| {
            let mut page = GamePage::local(seeded(3));
            page.act(|ctx| BoardCamera::new(ctx.ui_state).stage_measured(stage));
            page.with(|ctx| BoardCamera::new(ctx.ui_state).pose().zoom)
        };
        assert_eq!(zoom_on(Rect::new(0.0, 52.0, 390.0, 652.0)), 1.0);
        let hd = Rect::new(0.0, 75.0, 1477.0, 804.0);
        assert!((zoom_on(hd) - home_zoom(Size::of(&hd))).abs() < 1e-9);
        assert!(zoom_on(hd) > 1.4);
    }

    #[test]
    fn a_camera_at_home_follows_the_stage_as_it_resizes_and_one_the_player_zoomed_stays() {
        let small = Rect::new(0.0, 52.0, 985.0, 526.0);
        let hd = Rect::new(0.0, 75.0, 1477.0, 804.0);
        let zoom_after = |player_zooms: bool| {
            let mut page = GamePage::local(seeded(3));
            page.act(|ctx| {
                let camera = BoardCamera::new(ctx.ui_state);
                camera.stage_measured(small);
                if player_zooms {
                    camera.zoom_in();
                }
                camera.stage_measured(hd);
            });
            page.with(|ctx| BoardCamera::new(ctx.ui_state).pose().zoom)
        };
        assert!((zoom_after(false) - home_zoom(Size::of(&hd))).abs() < 1e-9);
        // 1.25: what the player chose, not a home zoom of either stage.
        assert_eq!(zoom_after(true), 1.25);
    }

    #[test]
    fn reset_goes_back_to_the_stages_home_zoom() {
        let hd = Rect::new(0.0, 75.0, 1477.0, 804.0);
        let mut page = GamePage::local(seeded(3));
        page.act(|ctx| {
            let camera = BoardCamera::new(ctx.ui_state);
            camera.stage_measured(hd);
            camera.zoom_out();
            camera.reset_zoom();
        });
        let pose = page.with(|ctx| BoardCamera::new(ctx.ui_state).pose());
        assert_eq!(pose, Pose::HOME.with_zoom(home_zoom(Size::of(&hd))));
    }

    // ---- framing the board

    const STAGE: Rect = Rect {
        min_x: 0.0,
        min_y: 52.0,
        max_x: 390.0,
        max_y: 652.0,
    };

    fn positions_of(game: &GameState) -> Vec<Position> {
        game.board().iter().map(|m| m.position).collect()
    }

    #[test]
    fn fit_frames_the_whole_board() {
        let mut game = seeded(3);
        for _ in 0..20 {
            game.skip_stuck_players();
            let Some(m) = game.legal_moves().first().copied() else {
                break;
            };
            game.play_move(m.placement()).unwrap();
        }
        let placed = positions_of(&game);
        let mut page = GamePage::local(game);
        page.act(|ctx| {
            ctx.ui_state.stage().set(Some(STAGE));
            let camera = BoardCamera::new(ctx.ui_state);
            camera.set_pose(Pose {
                zoom: 4.0,
                pan_x: 300.0,
                pan_y: -200.0,
            });
            camera.fit_board(&placed);
        });
        let pose = page.with(|ctx| BoardCamera::new(ctx.ui_state).pose());
        assert!(shows(
            hexes_rect(&placed, HEX_SIZE).unwrap(),
            pose,
            board_center(&placed, HEX_SIZE),
            Size::of(&STAGE),
            FIT_PAD,
        ));
        assert!(pose.zoom <= 1.3, "{pose:?}");
    }

    #[test]
    fn framing_waits_for_the_stage_to_be_measured() {
        let (game, idx, _) = tile_with_positions(|_| true);
        let placed = positions_of(&game);
        let spots: Vec<Position> = game.hand_info()[idx].positions.keys().copied().collect();
        let mut page = GamePage::local(game);
        page.act(|ctx| {
            ctx.ui_state.zoom().set(2.0);
            let camera = BoardCamera::new(ctx.ui_state);
            camera.frame_board(&placed);
            camera.frame_spots(&placed, &spots);
        });
        assert_eq!(
            page.with(|ctx| BoardCamera::new(ctx.ui_state).pose().zoom),
            2.0
        );
    }

    #[test]
    fn choosing_a_tile_brings_its_spots_into_view_at_a_tappable_size() {
        let (game, idx, _) = tile_with_positions(|_| true);
        let placed = positions_of(&game);
        let spots: Vec<Position> = game.hand_info()[idx].positions.keys().copied().collect();
        let size = Size::of(&STAGE);
        let center = board_center(&placed, HEX_SIZE);
        let spots_rect = hexes_rect(&spots, HEX_SIZE).unwrap();

        // From a camera that is zoomed in on nothing in particular, and from
        // one zoomed out too far to tap.
        for start in [
            Pose {
                zoom: 3.0,
                pan_x: 900.0,
                pan_y: -700.0,
            },
            Pose::HOME.with_zoom(MIN_ZOOM),
        ] {
            let mut page = GamePage::local(game.clone());
            page.act(|ctx| {
                ctx.ui_state.stage().set(Some(STAGE));
                let camera = BoardCamera::new(ctx.ui_state);
                camera.set_pose(start);
                camera.frame_spots(&placed, &spots);
            });
            let pose = page.with(|ctx| BoardCamera::new(ctx.ui_state).pose());
            assert!(
                shows(spots_rect, pose, center, size, FIT_PAD),
                "{start:?} → {pose:?}"
            );
            assert!(spot_width(pose.zoom) >= MIN_TARGET, "{pose:?}");
        }
    }

    #[test]
    fn choosing_a_tile_leaves_a_camera_that_already_shows_the_spots() {
        let (game, idx, _) = tile_with_positions(|_| true);
        let placed = positions_of(&game);
        let spots: Vec<Position> = game.hand_info()[idx].positions.keys().copied().collect();
        let mut page = GamePage::local(game);
        let pose = Pose {
            zoom: 0.9,
            pan_x: 0.0,
            pan_y: 0.0,
        };
        page.act(|ctx| {
            ctx.ui_state.stage().set(Some(STAGE));
            let camera = BoardCamera::new(ctx.ui_state);
            camera.set_pose(pose);
            camera.frame_spots(&placed, &spots);
        });
        let framed = page.with(|ctx| BoardCamera::new(ctx.ui_state).pose());
        // The sample positions fit a phone's stage at 90%.
        let size = Size::of(&STAGE);
        let center = board_center(&placed, HEX_SIZE);
        let on_screen = shows(
            hexes_rect(&spots, HEX_SIZE).unwrap(),
            pose,
            center,
            size,
            FIT_PAD,
        );
        assert_eq!(framed == pose, on_screen);
    }

    #[test]
    fn after_a_move_the_board_fits_the_stage_at_no_less_than_a_tappable_zoom() {
        let mut game = seeded(3);
        // A long game: the board outgrows a phone's stage at 100%.
        for _ in 0..30 {
            game.skip_stuck_players();
            let Some(m) = game.legal_moves().first().copied() else {
                break;
            };
            game.play_move(m.placement()).unwrap();
        }
        let placed = positions_of(&game);
        let rect = hexes_rect(&placed, HEX_SIZE).unwrap();
        let mut page = GamePage::local(game);
        page.act(|ctx| {
            ctx.ui_state.stage().set(Some(STAGE));
            let camera = BoardCamera::new(ctx.ui_state);
            camera.set_pose(Pose {
                zoom: 1.0,
                pan_x: 123.0,
                pan_y: -45.0,
            });
            camera.frame_board(&placed);
        });
        let pose = page.with(|ctx| BoardCamera::new(ctx.ui_state).pose());
        assert_eq!((pose.pan_x, pose.pan_y), (0.0, 0.0));
        assert!(pose.zoom >= MIN_SPOT_ZOOM && pose.zoom <= 1.0, "{pose:?}");
        let on_stage = rect_to_stage(
            rect,
            pose,
            board_center(&placed, HEX_SIZE),
            Size::of(&STAGE),
        );
        // Whole on the stage unless even the tappable zoom cannot show it.
        assert!(
            pose.zoom == MIN_SPOT_ZOOM
                || (on_stage.width() <= STAGE.width() && on_stage.height() <= STAGE.height()),
            "{pose:?} {on_stage:?}"
        );
    }
}
