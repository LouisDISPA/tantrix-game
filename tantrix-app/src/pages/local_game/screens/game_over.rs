use dioxus::prelude::*;

use crate::pages::local_game::{LocalGameContext, Screen};
use crate::route::Route;
use crate::state::ui_state::UiState;
use crate::svg::paths::color_css_var;

#[component]
pub fn GameOver() -> Element {
    let nav = navigator();
    let mut ctx = use_context::<LocalGameContext>();

    let gs = ctx.game_state.read();
    let active_players: Vec<tantrix_game::Color> = gs.playing_colors().to_vec();
    let scores = gs.game.scores();

    let winner_color = active_players
        .iter()
        .copied()
        .max_by_key(|&c| scores[c].score)
        .unwrap_or(tantrix_game::Color::Red);
    drop(gs);

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
                                let score_text = if ps.longest_loop > 0 && ps.longest_loop * 2 >= ps.longest_line {
                                    format!("{} pts (loop: {})", ps.score, ps.longest_loop)
                                } else if ps.longest_line > 0 {
                                    format!("{} pts (line: {})", ps.score, ps.longest_line)
                                } else {
                                    "0 pts".to_string()
                                };
                                let is_winner = *color == winner_color;
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
                                        if is_winner {
                                            span { style: "font-weight: 700; color: {cv};", "{score_text}" }
                                        } else {
                                            span { "{score_text}" }
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
