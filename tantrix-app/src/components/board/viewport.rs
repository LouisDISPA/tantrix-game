use dioxus::prelude::*;
use tantrix_game::{Move, Position};

use crate::hooks::camera::camera_style;
use crate::hooks::use_board_camera;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{UiStateStoreExt, UiStateStoreImplExt};
use crate::svg::geometry::{board_bounding_box, HEX_SIZE};

/// The zoom/pan viewport that hosts all board layers.
///
/// Owns the CSS transform, drag/wheel/right-click handlers, and the inner
/// recentering translation so the bounding box of placed tiles stays centered.
/// Layers (`BoardTilesLayer`, `BoardEmptyLayer`, `BoardPreview`) are passed
/// as children; `placed_moves` is the same list the tiles layer draws.
///
/// Each value it shows is a memo, so a drag re-renders this component (its
/// transform moves) and nothing else; the layers inside are not re-rendered.
#[component]
pub fn BoardViewport(placed_moves: ReadSignal<Vec<Move>>, children: Element) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("BoardViewport", ());
    let ctx = use_context::<GameCtx>();
    let mut ui_state = ctx.ui_state;
    let camera = use_board_camera();

    let transform =
        use_memo(move || camera_style(ui_state.zoom()(), ui_state.pan_x()(), ui_state.pan_y()()));
    // A drag rewrites `drag` on every mousemove; only its start and end
    // change the class.
    let dragging = use_memo(move || ui_state.drag()().is_active());

    // Recenter the tiles layer on the bounding box of placed moves so new
    // tiles push outward from the middle rather than off-screen.
    let center = use_memo(move || {
        let positions: Vec<Position> = placed_moves.read().iter().map(|m| m.position).collect();
        let (x, y) = match board_bounding_box(&positions, HEX_SIZE) {
            Some((min_x, min_y, max_x, max_y)) => ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0),
            None => (0.0, 0.0),
        };
        format!("--center-x: {x:.1}px; --center-y: {y:.1}px;")
    });

    rsx! {
        div {
            class: "board-viewport",
            class: if dragging() { "dragging" },
            style: "{transform}",

            onwheel: move |evt| camera.on_wheel(evt.data().delta().strip_units().y),
            onmousedown: move |evt| {
                let at = evt.data().client_coordinates();
                camera.drag_start(at.x, at.y);
            },
            onmousemove: move |evt| {
                let at = evt.data().client_coordinates();
                camera.drag_to(at.x, at.y);
            },
            onmouseup: move |_| camera.drag_end(),
            onmouseleave: move |_| camera.drag_end(),
            oncontextmenu: move |evt| {
                evt.prevent_default();
                ui_state.go_back();
            },

            div { class: "board-tiles-layer", style: "{center}", {children} }
        }
    }
}
