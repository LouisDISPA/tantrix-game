use dioxus::prelude::*;

use crate::state::game_state::GameCtx;
use crate::state::ui_state::{set_if_changed, Drag, UiState, UiStateStoreExt};

const MIN_ZOOM: f64 = 0.3;
const MAX_ZOOM: f64 = 5.0;

/// The board camera's controls: zoom in/out, reset, wheel zoom and
/// drag-to-pan. They write the camera fields of `UiState` one at a time
/// (`zoom`, `pan_x`, `pan_y`, `drag`), so a drag's mousemoves wake the
/// readers of the pan only, never the rest of the board.
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

    pub fn zoom_in(self) {
        self.zoom_with(|zoom| zoom * 1.25);
    }

    pub fn zoom_out(self) {
        self.zoom_with(|zoom| zoom / 1.25);
    }

    /// Back to 100%, centered.
    pub fn reset_zoom(self) {
        set_if_changed(self.ui_state.zoom(), 1.0);
        set_if_changed(self.ui_state.pan_x(), 0.0);
        set_if_changed(self.ui_state.pan_y(), 0.0);
    }

    /// Zoom in a step for a wheel scrolled up (`delta_y < 0`), out otherwise.
    pub fn on_wheel(self, delta_y: f64) {
        let factor = if delta_y < 0.0 { 1.1 } else { 0.9 };
        self.zoom_with(|zoom| zoom * factor);
    }

    /// The pointer went down at (`x`, `y`): start panning from there.
    pub fn drag_start(self, x: f64, y: f64) {
        self.ui_state.drag().set(Drag::Active {
            last_x: x,
            last_y: y,
        });
    }

    /// The pointer moved to (`x`, `y`): pan by the distance from the last
    /// position while a drag is on.
    pub fn drag_to(self, x: f64, y: f64) {
        let ui = self.ui_state;
        let Drag::Active { last_x, last_y } = *ui.drag().peek() else {
            return;
        };
        let zoom = *ui.zoom().peek();
        let pan_x = *ui.pan_x().peek() - (x - last_x) / zoom;
        let pan_y = *ui.pan_y().peek() - (y - last_y) / zoom;
        set_if_changed(ui.pan_x(), pan_x);
        set_if_changed(ui.pan_y(), pan_y);
        // Its readers only look at whether a drag is on, through a memo.
        ui.drag().set(Drag::Active {
            last_x: x,
            last_y: y,
        });
    }

    pub fn drag_end(self) {
        set_if_changed(self.ui_state.drag(), Drag::Idle);
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

    use super::{camera_style, zoom_percent, BoardCamera, MAX_ZOOM, MIN_ZOOM};
    use crate::state::test_games::seeded;
    use crate::state::ui_state::UiStateStoreExt;
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
            camera.drag_start(10.0, 10.0);
            camera.drag_to(41.25, -5.625);
            camera.drag_end();
            // After the drag ends, moving the pointer pans nothing.
            camera.drag_to(500.0, 500.0);
        });
        assert_eq!(pose, (1.5625, -20.0, 10.0));
    }

    #[test]
    fn resetting_the_zoom_recentres_the_board() {
        let pose = camera_after(|camera| {
            camera.zoom_out();
            camera.drag_start(0.0, 0.0);
            camera.drag_to(30.0, 40.0);
            camera.drag_end();
            camera.reset_zoom();
        });
        assert_eq!(pose, (1.0, 0.0, 0.0));
    }
}
