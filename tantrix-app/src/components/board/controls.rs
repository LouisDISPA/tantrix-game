use dioxus::prelude::*;
use tantrix_net::SessionCommand;

use crate::hooks::use_board_camera;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};

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
#[component]
pub fn EndGameButton() -> Element {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;
    let can_end = use_memo(move || ctx.online.read().is_none() || ctx.my_color.read().is_some());

    if !can_end() {
        return rsx! {};
    }

    rsx! {
        div { class: "board-controls",
            button {
                class: "board-ctrl-btn end-game-btn",
                title: "End game",
                onclick: move |_| {
                    if let Some(sender) = ctx.online.read().as_ref() {
                        sender.send(SessionCommand::EndGame);
                    } else {
                        ui_state.phase().set(TurnPhase::GameOver);
                        ui_state.message().set("Game ended early.".to_string());
                    }
                },
                "End Game"
            }
        }
    }
}
