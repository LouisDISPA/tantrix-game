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
        div { class: "title-screen",
            div { class: "title-bg" }
            div { class: "title-glow" }

            div { class: "floating-hexes",
                div { class: "floating-hex hex-red",
                    div { class: "floating-hex-inner" }
                }
                div { class: "floating-hex hex-blue",
                    div { class: "floating-hex-inner" }
                }
                div { class: "floating-hex hex-yellow",
                    div { class: "floating-hex-inner" }
                }
                div { class: "floating-hex hex-green",
                    div { class: "floating-hex-inner" }
                }
            }

            div { class: "title-content",
                div { class: "setup-card",
                    div { class: "setup-hex-frame" }
                    button {
                        class: "setup-back-btn",
                        onclick: move |_| { let _ = nav.push(Route::MainMenu {}); },
                        "\u{2190} Back"
                    }
                    div { class: "setup-title", "New Game" }
                    div { class: "setup-subtitle", "Choose number of players" }

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
                        class: "setup-start-btn",
                        onclick: move |_| {
                            *GAME_CONFIG.write() = Some(GameConfig::new(count));
                            let _ = nav.push(Route::GameScreen {});
                        },
                        "Start Game"
                    }
                }
            }

            button {
                class: "theme-toggle",
                onclick: move |_| toggle_theme(),
                "{THEME.read().icon()}"
            }
        }
    }
}
