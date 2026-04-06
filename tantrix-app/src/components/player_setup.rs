use dioxus::prelude::*;

use crate::route::Route;
use crate::state::config::{GameConfig, GAME_CONFIG};
use crate::state::theme::{toggle_theme, THEME};
use crate::svg::paths::color_css_var;
use tantrix_game::Color;

#[component]
pub fn PlayerSetup() -> Element {
    let mut player_count = use_signal(|| 2usize);
    let nav = navigator();

    let colors = [Color::Red, Color::Blue, Color::Yellow, Color::Green];
    let count = *player_count.read();

    rsx! {
        div { class: "screen",
            button {
                class: "theme-toggle",
                onclick: move |_| toggle_theme(),
                "{THEME.read().icon()}"
            }
            div { class: "card",
                div { class: "card-title", "New Game" }
                div { class: "card-subtitle", "Choose number of players" }

                div { class: "player-count-selector",
                    for n in 2..=4 {
                        button {
                            class: if n == count { "count-btn active" } else { "count-btn" },
                            onclick: move |_| player_count.set(n),
                            "{n}"
                        }
                    }
                }

                div { class: "player-slots",
                    for (i, color) in colors.iter().enumerate() {
                        {
                            let is_active = i < count;
                            let color_var = color_css_var(*color);
                            rsx! {
                                div {
                                    class: "player-slot",
                                    style: if is_active { "opacity: 1;" },
                                    div {
                                        class: "color-swatch",
                                        style: "background-color: {color_var}",
                                    }
                                    div { class: "player-label", "{color}" }
                                    div { class: "player-status",
                                        if is_active { "Player" } else { "Empty" }
                                    }
                                }
                            }
                        }
                    }
                }

                button {
                    class: "btn btn-primary btn-large",
                    style: "width: 100%",
                    onclick: move |_| {
                        *GAME_CONFIG.write() = Some(GameConfig::new(count));
                        let _ = nav.push(Route::GameScreen {});
                    },
                    "Start Game"
                }
            }
        }
    }
}
