use dioxus::prelude::*;

use crate::pages::local_game::{LocalGameContext, Screen};
use crate::route::Route;
use crate::state::game_state::GameStateStoreExt;
use crate::state::ui_state::UiState;
use crate::svg::paths::color_css_var;

#[component]
pub fn GameOver() -> Element {
    let nav = navigator();
    let mut ctx = use_context::<LocalGameContext>();

    // Subscribe to individual fields for fine-grained reactivity
    let active_players: Vec<tantrix_game::Color> = ctx
        .game_state
        .turn_tracker()
        .read()
        .playing_colors()
        .to_vec();
    let scores = ctx.game_state.board().read().scores();

    let winner_color = active_players
        .iter()
        .copied()
        .max_by_key(|&c| scores[c].score)
        .unwrap_or(tantrix_game::Color::Red);

    let color_var = color_css_var(winner_color);

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

            div { class: "game-over-overlay",
                div { class: "game-over-backdrop" }
                div { class: "game-over-card",
                    div { class: "game-over-hex-frame" }
                    div { class: "winner-title", "Winner" }
                    div {
                        class: "winner-name",
                        style: "color: {color_var}; --glow-color: {color_var}",
                        "{winner_color}"
                    }
                    div { class: "game-over-stats",
                        for color in active_players.iter() {
                            {
                                let cv = color_css_var(*color);
                                let ps = scores[*color];
                                let is_winner = *color == winner_color;
                                let row_class = if is_winner { "stat-row stat-row-winner" } else { "stat-row" };
                                let detail = if ps.longest_loop > 0 && ps.longest_loop * 2 >= ps.longest_line {
                                    format!("loop of {}", ps.longest_loop)
                                } else if ps.longest_line > 0 {
                                    format!("line of {}", ps.longest_line)
                                } else {
                                    String::new()
                                };
                                rsx! {
                                    div {
                                        class: "{row_class}",
                                        style: if is_winner { format!("--player-color: {cv}") } else { String::new() },
                                        div { class: "stat-player",
                                            div { class: "stat-color", style: "background-color: {cv}" }
                                            span { class: "stat-name", "{color}" }
                                        }
                                        div { class: "stat-score",
                                            span {
                                                class: "stat-points",
                                                style: if is_winner { format!("color: {cv}") } else { String::new() },
                                                "{ps.score}"
                                            }
                                            if !detail.is_empty() {
                                                span { class: "stat-detail", "{detail}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "game-over-buttons",
                        button {
                            class: "game-over-btn game-over-btn-primary",
                            onclick: move |_| {
                                ctx.ui_state.set(UiState::default());
                                ctx.screen.set(Screen::Setup);
                            },
                            "Play Again"
                        }
                        button {
                            class: "game-over-btn game-over-btn-secondary",
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
