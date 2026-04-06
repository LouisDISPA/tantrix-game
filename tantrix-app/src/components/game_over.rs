use dioxus::prelude::*;
use rand::prelude::IndexedRandom;

use crate::route::Route;
use crate::state::config::GAME_CONFIG;
use crate::state::theme::{toggle_theme, THEME};
use crate::svg::paths::color_css_var;

#[component]
pub fn GameOver() -> Element {
    let nav = navigator();

    let config = GAME_CONFIG.read();
    let active_players: Vec<tantrix_game::Color> = config
        .as_ref()
        .map(|c| {
            c.players
                .iter()
                .filter(|(_, p)| *p != tantrix_game::Player::None)
                .map(|(c, _)| *c)
                .collect()
        })
        .unwrap_or_default();

    // Pick random winner
    let winner = use_signal(|| {
        let mut rng = rand::rng();
        active_players
            .as_slice()
            .choose(&mut rng)
            .copied()
            .unwrap_or(tantrix_game::Color::Red)
    });

    let winner_color = *winner.read();
    let color_var = color_css_var(winner_color);

    rsx! {
        div { class: "screen",
            button {
                class: "theme-toggle",
                onclick: move |_| toggle_theme(),
                "{THEME.read().icon()}"
            }
            div { class: "game-over-overlay",
                div { class: "game-over-card",
                    div { class: "winner-title", "Winner!" }
                    div {
                        class: "winner-name",
                        style: "color: {color_var}; --glow-color: {color_var}",
                        "{winner_color}"
                    }
                    div { class: "game-over-stats",
                        for color in active_players.iter() {
                            {
                                let cv = color_css_var(*color);
                                rsx! {
                                    div { class: "stat-row",
                                        div {
                                            style: "display: flex; align-items: center; gap: 8px;",
                                            div {
                                                class: "stat-color",
                                                style: "background-color: {cv}",
                                            }
                                            "{color}"
                                        }
                                        if *color == winner_color {
                                            span { style: "font-weight: 700; color: {cv};", "Winner!" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "game-over-buttons",
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| {
                                let _ = nav.push(Route::PlayerSetup {});
                            },
                            "Play Again"
                        }
                        button {
                            class: "btn btn-secondary",
                            onclick: move |_| {
                                let _ = nav.push(Route::MainMenu {});
                            },
                            "Main Menu"
                        }
                    }
                }
            }
        }
    }
}
