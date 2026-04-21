use dioxus::prelude::*;
use tantrix_game::Color;

use crate::pages::local_game::{LocalGameContext, Screen};
use crate::route::Route;
use crate::state::game_state::{determine_winners, GameStateStoreExt};
use crate::state::ui_state::{UiState, UiStateStoreExt};
use crate::svg::paths::color_css_var;

// ---------------------------------------------------------------------------
// ScoreRow subcomponent
// ---------------------------------------------------------------------------

#[component]
fn ScoreRow(
    color: ReadSignal<Color>,
    score: ReadSignal<usize>,
    is_winner: ReadSignal<bool>,
    is_active: ReadSignal<bool>,
    on_toggle: EventHandler<Color>,
    on_hover: EventHandler<Option<Color>>,
) -> Element {
    let c = color();
    let score = score();
    let is_winner = is_winner();
    let is_active = is_active();
    let cv = color_css_var(c);

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
            onclick: move |_| on_toggle.call(c),
            onmouseenter: move |_| on_hover.call(Some(c)),
            onmouseleave: move |_| on_hover.call(None),
            div { class: "results-player",
                div { class: "results-color-swatch", style: "background-color: {cv}" }
                span { class: "results-name", "{c}" }
            }
            span {
                class: "results-points",
                style: if is_winner { format!("color: {cv}") } else { String::new() },
                "{score}"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GameOverOverlay
// ---------------------------------------------------------------------------

#[component]
pub fn GameOverOverlay() -> Element {
    let nav = navigator();
    let mut ctx = use_context::<LocalGameContext>();
    let ui_state = ctx.ui_state;

    // Subscribe to individual fields for fine-grained reactivity
    let active_players: Vec<Color> = ctx
        .game_state
        .turn_tracker()
        .read()
        .playing_colors()
        .to_vec();
    let scores = ctx.game_state.board().read().scores();
    let (winners, is_tie) = determine_winners(&active_players, &scores);

    // Subscribe only to highlighted_color field
    let highlighted = ui_state.highlighted_color()();

    rsx! {
        div { class: "board-results-overlay",
            div { class: "board-results-card",
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
                    for &c in active_players.iter() {
                        ScoreRow {
                            key: "{c:?}",
                            color: c,
                            score: scores[c].score,
                            is_winner: winners.contains(&c),
                            is_active: highlighted == Some(c),
                            on_toggle: move |color: Color| {
                                let current = *ui_state.highlighted_color().read();
                                if current == Some(color) {
                                    ui_state.highlighted_color().set(None);
                                } else {
                                    ui_state.highlighted_color().set(Some(color));
                                }
                            },
                            on_hover: move |color: Option<Color>| {
                                ui_state.hovered_scoring_color().set(color);
                            },
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
