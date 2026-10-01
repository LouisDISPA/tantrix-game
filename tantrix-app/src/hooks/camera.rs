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
