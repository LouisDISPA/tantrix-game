use dioxus::prelude::*;

use crate::components::board::Board;
use crate::components::scores_panel::ScoresPanel;
use crate::pages::local_game::{LocalGameContext, Screen};
use crate::route::Route;
use crate::state::ui_state::UiState;

/// End-of-game screen. Reuses [`Board`] so the final tile layout and scoring
/// highlights stay visible, with the scores card overlaid on top.
///
/// The two exit paths (Play Again / Main Menu) are intentionally kept
/// separate: Play Again resets UI state and returns to the setup screen;
/// Main Menu uses the router to leave the local-game page entirely.
#[component]
pub fn GameOver() -> Element {
    let nav = navigator();
    let mut ctx = use_context::<LocalGameContext>();

    rsx! {
        div {
            class: "game-layout game-over-layout",
            style: "display: grid; grid-template-rows: 1fr; grid-template-columns: 1fr; grid-template-areas: \"board\"; height: 100vh; width: 100vw;",

            div { style: "grid-area: board; min-width: 0; min-height: 0;", Board {} }

            ScoresPanel {
                button {
                    class: "results-btn results-btn-primary",
                    onclick: move |_| {
                        ctx.ui_state.set(UiState::default());
                        ctx.screen.set(Screen::Setup);
                    },
                    "Play Again"
                }
                button {
                    class: "results-btn results-btn-secondary",
                    onclick: move |_| {
                        let _ = nav.push(Route::MainMenu {});
                    },
                    "Main Menu"
                }
            }
        }
    }
}
