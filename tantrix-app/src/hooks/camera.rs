use dioxus::prelude::*;

use crate::state::game_state::GameContext;
use crate::state::ui_state::{UiState, UiStateStoreExt};

/// Immutable display data for the current camera pose.
pub struct BoardCamera {
    pub transform: String,
    pub zoom_percent: i32,
    pub viewport_class: String,
}

/// Camera event handlers. Holds a `Copy`able `Store` so each method just
/// copies it out of `self` — satisfies Dioxus's ownership rules for closures
/// attached to event handlers in RSX.
#[derive(Clone, Copy)]
pub struct BoardCameraHandlers {
    ui_state: Store<UiState>,
}

impl BoardCameraHandlers {
    pub fn zoom_in(self) {
        let z = *self.ui_state.zoom().read();
        self.ui_state.zoom().set((z * 1.25).clamp(0.3, 5.0));
    }

    pub fn zoom_out(self) {
        let z = *self.ui_state.zoom().read();
        self.ui_state.zoom().set((z / 1.25).clamp(0.3, 5.0));
    }

    pub fn reset_zoom(mut self) {
        let mut ui = self.ui_state.write();
        ui.zoom = 1.0;
        ui.pan_x = 0.0;
        ui.pan_y = 0.0;
    }

    pub fn on_wheel(self, evt: &WheelEvent) {
        let delta_y = evt.data().delta().strip_units().y;
        let z = *self.ui_state.zoom().read();
        let factor = if delta_y < 0.0 { 1.1 } else { 0.9 };
        self.ui_state.zoom().set((z * factor).clamp(0.3, 5.0));
    }

    pub fn on_drag_start(mut self, evt: &MouseEvent) {
        let mut ui = self.ui_state.write();
        let coords = evt.data().client_coordinates();
        ui.dragging = true;
        ui.last_drag_x = coords.x;
        ui.last_drag_y = coords.y;
    }

    pub fn on_drag_move(self, evt: &MouseEvent) {
        let mut ui = self.ui_state;
        if *ui.dragging().read() {
            let coords = evt.data().client_coordinates();
            let last_x = *ui.last_drag_x().read();
            let last_y = *ui.last_drag_y().read();
            let z = *ui.zoom().read();
            let dx = coords.x - last_x;
            let dy = coords.y - last_y;
            let mut w = ui.write();
            w.pan_x -= dx / z;
            w.pan_y -= dy / z;
            w.last_drag_x = coords.x;
            w.last_drag_y = coords.y;
        }
    }

    pub fn on_drag_end(self) {
        self.ui_state.dragging().set(false);
    }
}

/// Read the current zoom/pan/drag state and expose it plus a bundle of event
/// handlers. Components that need the CSS transform read this — everything
/// that needs to *mutate* the camera uses the handler struct.
pub fn use_board_camera() -> (BoardCamera, BoardCameraHandlers) {
    let ctx = use_context::<GameContext>();
    let ui_state = ctx.ui_state;

    let zoom = ui_state.zoom()();
    let pan_x = ui_state.pan_x()();
    let pan_y = ui_state.pan_y()();
    let dragging = ui_state.dragging()();

    let transform = format!(
        "transform: scale({:.3}) translate({:.1}px, {:.1}px);",
        zoom, -pan_x, -pan_y
    );

    let mut viewport_class = String::from("board-viewport");
    if dragging {
        viewport_class.push_str(" dragging");
    }

    (
        BoardCamera {
            transform,
            zoom_percent: (zoom * 100.0) as i32,
            viewport_class,
        },
        BoardCameraHandlers { ui_state },
    )
}
