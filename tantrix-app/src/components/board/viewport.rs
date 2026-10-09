use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use tantrix_game::{Move, Position};

use crate::hooks::camera::camera_style;
use crate::hooks::use_board_camera;
use crate::state::game_state::GameCtx;
use crate::state::gesture::{MOUSE_SLOP, TOUCH_SLOP};
use crate::state::tile_drag::holds_tile_pointer;
use crate::state::ui_state::{TurnPhase, UiStateStoreExt, UiStateStoreImplExt};
use crate::svg::geometry::{board_center, HEX_SIZE};

/// The zoom/pan viewport that hosts all board layers.
///
/// Owns the CSS transform, the pointer (drag, pinch), wheel and right-click
/// handlers, and the inner recentering translation so the bounding box of
/// placed tiles stays centered.
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
    // A drag rewrites `gesture` on every pointer move; only its start and
    // end change the class (a tap that stays within the slop is no drag).
    let dragging = use_memo(move || ui_state.gesture()().is_drag());
    let tile_dragging =
        use_memo(move || ui_state.tile_drag()().is_some_and(|drag| drag.is_dragging()));

    // Recenter the tiles layer on the bounding box of placed moves so new
    // tiles push outward from the middle rather than off-screen.
    let center = use_memo(move || {
        let positions: Vec<Position> = placed_moves.read().iter().map(|m| m.position).collect();
        let (x, y) = board_center(&positions, HEX_SIZE);
        format!("--center-x: {x:.1}px; --center-y: {y:.1}px;")
    });

    rsx! {
        div {
            class: "board-viewport",
            class: if dragging() { "dragging" },
            class: if tile_dragging() { "tile-dragging" },
            style: "{transform}",

            onwheel: move |evt| {
                if !holds_tile_pointer(ctx) {
                    camera.on_wheel(evt.data().delta().strip_units().y);
                }
            },
            onpointerdown: move |evt| {
                if holds_tile_pointer(ctx) { return; }
                // The primary button (or a touch) pans; the right one is "back".
                if evt.data().trigger_button() == Some(MouseButton::Primary) {
                    let at = evt.data().client_coordinates();
                    let slop = if evt.data().pointer_type() == "mouse" {
                        MOUSE_SLOP
                    } else {
                        TOUCH_SLOP
                    };
                    camera.pointer_down(evt.data().pointer_id(), at.x, at.y, slop);
                }
            },
            onpointermove: move |evt| {
                if holds_tile_pointer(ctx) { return; }
                // A mouse that moves with no button held let go somewhere we
                // did not see (outside the window, over a dialog).
                if evt.data().pointer_type() == "mouse"
                    && !evt.data().held_buttons().contains(MouseButton::Primary)
                {
                    camera.pointer_up(evt.data().pointer_id());
                    return;
                }
                let at = evt.data().client_coordinates();
                camera.pointer_move(evt.data().pointer_id(), at.x, at.y);
            },
            onpointerup: move |evt| camera.pointer_up(evt.data().pointer_id()),
            onpointercancel: move |evt| camera.pointer_up(evt.data().pointer_id()),
            // A finger stays with the board once it is down (the browser
            // captures it); only a mouse can wander off.
            onpointerleave: move |evt| {
                if evt.data().pointer_type() == "mouse" {
                    camera.pointer_leave();
                }
            },
            // A tap on the empty board (a spot stops its own) closes the card
            // of options, back to choosing a spot.
            onclick: move |_| {
                if !camera.was_drag() && *ui_state.phase().peek() == TurnPhase::SelectRotation {
                    ui_state.go_back();
                }
            },
            oncontextmenu: move |evt| {
                evt.prevent_default();
                // The right button goes back; a long press on a touch screen
                // (which also opens this menu) does not.
                if evt.data().trigger_button() == Some(MouseButton::Secondary) {
                    ui_state.go_back();
                }
            },

            div { class: "board-tiles-layer", style: "{center}", {children} }
        }
    }
}
