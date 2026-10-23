use dioxus::prelude::*;

use super::game_hud::GameHud;
use crate::components::board::Board;
use crate::components::scores_panel::ScoresPanel;
use crate::route::Route;

/// End-of-game screen for local and online play. Reuses the HUD row and
/// [`Board`] so the final tile layout and scoring highlights stay visible,
/// with the scores card in the panel where the hands were.
///
/// `children` go in the HUD; `banners` show connection feedback online.
#[component]
pub fn GameOver(
    #[props(into)] again_label: String,
    on_again: EventHandler<()>,
    #[props(default)] again_disabled: bool,
    #[props(default = "Main Menu".to_string())] leave_label: String,
    #[props(default)] on_leave: Option<EventHandler<()>>,
    #[props(default = VNode::empty())] banners: Element,
    children: Element,
) -> Element {
    let nav = navigator();

    rsx! {
        div { class: "game-layout game-over-layout",

            GameHud { {children} }

            div { class: "board-slot", Board {} }

            div { class: "game-announcements", {banners} }

            ScoresPanel {
                button {
                    class: "button button-primary results-btn results-btn-primary",
                    disabled: again_disabled,
                    onclick: move |_| on_again.call(()),
                    {again_label}
                }
                button {
                    class: "button button-secondary results-btn results-btn-secondary",
                    onclick: move |_| {
                        if let Some(on_leave) = on_leave {
                            on_leave.call(());
                        } else {
                            let _ = nav.push(Route::MainMenu {});
                        }
                    },
                    {leave_label}
                }
            }
        }
    }
}
