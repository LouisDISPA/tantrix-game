use dioxus::prelude::*;

use crate::hooks::camera::zoom_percent;
use crate::hooks::use_board_camera;
use crate::state::actions::end_game;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::UiStateStoreExt;

/// Floating zoom buttons in the corner of the board. Reads the zoom level
/// only, so panning the board does not re-render them.
#[component]
pub fn ZoomControls() -> Element {
    #[cfg(test)]
    crate::util::render_count::record("ZoomControls", ());
    let ctx = use_context::<GameCtx>();
    let camera = use_board_camera();
    let percent = use_memo(move || zoom_percent(ctx.ui_state.zoom()()));

    rsx! {
        div { class: "zoom-controls",
            button {
                class: "zoom-btn",
                title: "Zoom in",
                onclick: move |_| camera.zoom_in(),
                "+"
            }
            button {
                class: "zoom-btn zoom-label",
                title: "Reset zoom",
                onclick: move |_| camera.reset_zoom(),
                "{percent}%"
            }
            button {
                class: "zoom-btn",
                title: "Zoom out",
                onclick: move |_| camera.zoom_out(),
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
