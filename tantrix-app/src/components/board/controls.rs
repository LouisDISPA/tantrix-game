use dioxus::prelude::*;
use tantrix_game::Position;

use crate::hooks::camera::zoom_percent;
use crate::hooks::use_board_camera;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::UiStateStoreExt;

/// The zoom buttons, and Fit, which frames the whole board. Reads the zoom
/// level only, so panning the board does not re-render them. On a touch
/// screen, where fingers pinch, the stylesheet keeps only Fit.
#[component]
pub fn ZoomControls() -> Element {
    #[cfg(test)]
    crate::util::render_count::record("ZoomControls", ());
    let ctx = use_context::<GameCtx>();
    let camera = use_board_camera();
    let percent = use_memo(move || zoom_percent(ctx.ui_state.zoom()()));

    rsx! {
        div { class: "zoom-controls",
            // The steps stop their click, so the menu they sit in stays open
            // for the next one.
            button {
                class: "button button-ghost segment zoom-btn zoom-step",
                title: "Zoom out",
                onclick: move |evt| {
                    evt.stop_propagation();
                    camera.zoom_out();
                },
                "−"
            }
            button {
                class: "button button-ghost segment zoom-btn zoom-label",
                title: "Reset zoom",
                onclick: move |evt| {
                    evt.stop_propagation();
                    camera.reset_zoom();
                },
                "{percent}%"
            }
            button {
                class: "button button-ghost segment zoom-btn zoom-step",
                title: "Zoom in",
                onclick: move |evt| {
                    evt.stop_propagation();
                    camera.zoom_in();
                },
                "+"
            }
            button {
                class: "button button-ghost segment zoom-btn zoom-fit",
                title: "Fit board",
                onclick: move |_| {
                    let placed: Vec<Position> = ctx
                        .game_state
                        .read()
                        .board()
                        .iter()
                        .map(|m| m.position)
                        .collect();
                    camera.fit_board(&placed);
                },
                "Fit"
            }
        }
    }
}

/// The End Game button: `on_click` asks for confirmation. Hidden from
/// someone watching a room, who cannot end its game.
#[component]
pub fn EndGameButton(on_click: EventHandler<()>) -> Element {
    let ctx = use_context::<GameCtx>();
    let can_end = use_memo(move || ctx.driver.can_end_game());

    if !can_end() {
        return rsx! {};
    }

    rsx! {
        button {
            class: "button button-ghost board-ctrl-btn end-game-btn",
            title: "End game",
            onclick: move |_| on_click.call(()),
            "End Game"
        }
    }
}
