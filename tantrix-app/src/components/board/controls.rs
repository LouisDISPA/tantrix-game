use dioxus::prelude::*;

use crate::hooks::use_board_camera;
use crate::state::actions::end_game;
use crate::state::game_state::GameCtx;

/// Floating zoom buttons in the corner of the board.
#[component]
pub fn ZoomControls() -> Element {
    let (camera, cam_handlers) = use_board_camera();

    rsx! {
        div { class: "zoom-controls",
            button {
                class: "zoom-btn",
                title: "Zoom in",
                onclick: move |_| cam_handlers.zoom_in(),
                "+"
            }
            button {
                class: "zoom-btn zoom-label",
                title: "Reset zoom",
                onclick: move |_| cam_handlers.reset_zoom(),
                "{camera.zoom_percent}%"
            }
            button {
                class: "zoom-btn",
                title: "Zoom out",
                onclick: move |_| cam_handlers.zoom_out(),
                "−"
            }
        }
    }
}

/// End local play immediately, or ask the room to end online play for everyone.
/// Hidden from someone watching a room.
#[component]
pub fn EndGameButton() -> Element {
    let ctx = use_context::<GameCtx>();
    let can_end = use_memo(move || ctx.driver.can_end_game());

    if !can_end() {
        return rsx! {};
    }

    rsx! {
        div { class: "board-controls",
            button {
                class: "board-ctrl-btn end-game-btn",
                title: "End game",
                onclick: move |_| end_game(ctx),
                "End Game"
            }
        }
    }
}
