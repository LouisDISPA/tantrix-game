use dioxus::prelude::*;
use tantrix_game::{Move, Position};

use crate::hooks::use_board_camera;
use crate::state::game_state::GameCtx;
use crate::svg::geometry::{board_bounding_box, HEX_SIZE};

/// The zoom/pan viewport that hosts all board layers.
///
/// Owns the CSS transform, drag/wheel/right-click handlers, and the inner
/// recentering translation so the bounding box of placed tiles stays centered.
/// Layers (`BoardTilesLayer`, `BoardEmptyLayer`) are passed as children;
/// `placed_moves` is the same list the tiles layer draws.
#[component]
pub fn BoardViewport(placed_moves: ReadSignal<Vec<Move>>, children: Element) -> Element {
    let ctx = use_context::<GameCtx>();
    let mut ui_state = ctx.ui_state;

    let (camera, cam_handlers) = use_board_camera();

    // Recenter the tiles layer on the bounding box of placed moves so new
    // tiles push outward from the middle rather than off-screen.
    let (center_x, center_y) = use_memo(move || {
        let positions: Vec<Position> = placed_moves.read().iter().map(|m| m.position).collect();
        if let Some((min_x, min_y, max_x, max_y)) = board_bounding_box(&positions, HEX_SIZE) {
            ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0)
        } else {
            (0.0, 0.0)
        }
    })();

    rsx! {
        div {
            class: "{camera.viewport_class}",
            style: "{camera.style}",

            onwheel: move |evt| cam_handlers.on_wheel(&evt),
            onmousedown: move |evt| cam_handlers.on_drag_start(&evt),
            onmousemove: move |evt| cam_handlers.on_drag_move(&evt),
            onmouseup: move |_| cam_handlers.on_drag_end(),
            onmouseleave: move |_| cam_handlers.on_drag_end(),
            oncontextmenu: move |evt| {
                evt.prevent_default();
                ui_state.write().go_back();
            },

            div {
                class: "board-tiles-layer",
                style: "--center-x: {center_x:.1}px; --center-y: {center_y:.1}px;",
                {children}
            }
        }
    }
}
