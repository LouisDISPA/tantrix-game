use dioxus::prelude::*;

use crate::pages::local_game::{LocalGameContext, Screen};
use crate::route::Route;
use crate::state::game_state::GameStateStoreExt;
use crate::state::ui_state::{UiState, UiStateStoreExt};
use crate::svg::paths::color_css_var;

#[component]
pub fn GameOverOverlay() -> Element {
    let nav = navigator();
    let mut ctx = use_context::<LocalGameContext>();
    let ui_state = ctx.ui_state;

    // Subscribe to individual fields for fine-grained reactivity
    let active_players: Vec<tantrix_game::Color> = ctx
        .game_state
        .turn_tracker()
        .read()
        .playing_colors()
        .to_vec();
    let scores = ctx.game_state.board().read().scores();

    let max_score = active_players
        .iter()
        .map(|&c| scores[c].score)
        .max()
        .unwrap_or(0);

    let winners: Vec<tantrix_game::Color> = active_players
        .iter()
        .copied()
        .filter(|&c| scores[c].score == max_score)
        .collect();

    let is_tie = winners.len() > 1;

    // Subscribe only to highlighted_color field
    let highlighted = ui_state.highlighted_color()();

    rsx! {
        div { class: "board-results-overlay",
            div { class: "board-results-card",
                // Hex frame behind the card
                div { class: "results-hex-frame" }

                div { class: "results-header",
                    if is_tie {
                        div { class: "results-winner-label", "Tie" }
                        div { class: "results-tie-names",
                            for (i , & color) in winners.iter().enumerate() {
                                {
                                    let cv = color_css_var(color);
                                    rsx! {
                                        if i > 0 {
                                            span { class: "results-tie-separator", " & " }
                                        }
                                        span { class: "results-winner-name", style: "color: {cv}; --glow-color: {cv}", "{color}" }
                                    }
                                }
                            }
                        }
                    } else {
                        {
                            let winner_color = winners[0];
                            let winner_var = color_css_var(winner_color);
                            rsx! {
                                div { class: "results-winner-label", "Winner" }
                                div {
                                    class: "results-winner-name",
                                    style: "color: {winner_var}; --glow-color: {winner_var}",
                                    "{winner_color}"
                                }
                            }
                        }
                    }
                }

                div { class: "results-divider" }

                div { class: "results-stats",
                    for color in active_players.iter() {
                        {
                            let c = *color;
                            let cv = color_css_var(c);
                            let ps = scores[c];
                            let is_winner = winners.contains(&c);
                            let is_active = highlighted == Some(c);
                            let row_class = match (is_winner, is_active) {
                                (true, true) => "results-row results-row-winner results-row-active",
                                (true, false) => "results-row results-row-winner",
                                (false, true) => "results-row results-row-active",
                                (false, false) => "results-row",
                            };
                            rsx! {
                                div {
                                    class: "{row_class}",
                                    style: format!("--player-color: {cv}"),
                                    onclick: move |_| {
                                        let current = *ui_state.highlighted_color().read();
                                        if current == Some(c) {
                                            ui_state.highlighted_color().set(None);
                                        } else {
                                            ui_state.highlighted_color().set(Some(c));
                                        }
                                    },
                                    onmouseenter: move |_| {
                                        ui_state.hovered_scoring_color().set(Some(c));
                                    },
                                    onmouseleave: move |_| {
                                        ui_state.hovered_scoring_color().set(None);
                                    },
                                    div { class: "results-player",
                                        div { class: "results-color-swatch", style: "background-color: {cv}" }
                                        span { class: "results-name", "{c}" }
                                    }
                                    span {
                                        class: "results-points",
                                        style: if is_winner { format!("color: {cv}") } else { String::new() },
                                        "{ps.score}"
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "results-buttons",
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
}
