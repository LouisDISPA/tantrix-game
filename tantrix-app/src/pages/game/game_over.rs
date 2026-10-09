use dioxus::prelude::*;

use super::game_hud::GameHud;
use crate::components::board::Board;
use crate::components::scores_panel::ScoresPanel;
use crate::route::Route;

/// End-of-game screen for local and online play. Reuses the HUD row and
/// [`Board`] so the final tile layout and scoring highlights stay visible,
/// with the scores card in the panel where the hands were.
///
/// The primary button leads to the next game, wherever the page starts one:
/// `again_label` / `on_again` ("Play Again" back to the local setup screen,
/// "Online Menu" online). Main Menu uses the router to leave the game page
/// entirely. `children` go in the HUD (the online room's badge).
#[component]
pub fn GameOver(
    #[props(into)] again_label: String,
    on_again: EventHandler<()>,
    children: Element,
) -> Element {
    let nav = navigator();

    rsx! {
        div { class: "game-layout game-over-layout",

            GameHud { {children} }

            div { class: "board-slot", Board {} }

            ScoresPanel {
                button {
                    class: "results-btn results-btn-primary",
                    onclick: move |_| on_again.call(()),
                    {again_label}
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
