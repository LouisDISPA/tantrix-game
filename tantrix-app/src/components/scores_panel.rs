use dioxus::prelude::*;
use tantrix_game::Color;

use crate::state::game_state::{determine_winners, GameContext, GameStateStoreExt};
use crate::state::ui_state::{Hovered, UiStateStoreExt};
use crate::svg::paths::color_css_var;

/// A single row in the scores list — one player's color, name, and score.
/// Clicking locks the scoring-path highlight to this color; hovering previews
/// it. Both interactions live in UiState so the board reacts accordingly.
#[component]
fn ScoreRow(
    color: ReadSignal<Color>,
    score: ReadSignal<usize>,
    is_winner: ReadSignal<bool>,
    is_active: ReadSignal<bool>,
) -> Element {
    let c = color();
    let score = score();
    let is_winner = is_winner();
    let is_active = is_active();
    let cv = color_css_var(c);

    let ctx = use_context::<GameContext>();
    let ui_state = ctx.ui_state;

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
            onmouseenter: move |_| ui_state.hovered().set(Hovered::ScoringColor(c)),
            onmouseleave: move |_| ui_state.hovered().set(Hovered::None),
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

/// End-of-game scores card: winner header, per-player rows, and a slot for
/// the parent screen to inject its own action buttons (Play Again, Main Menu).
#[component]
pub fn ScoresPanel(children: Element) -> Element {
    let ctx = use_context::<GameContext>();
    let game_state = ctx.game_state;
    let ui_state = ctx.ui_state;

    let active_players: Vec<Color> = game_state
        .turn_tracker()
        .read()
        .playing_colors()
        .to_vec();
    let scores = game_state.board().read().scores();
    let (winners, is_tie) = determine_winners(&active_players, &scores);
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
                                        span {
                                            class: "results-winner-name",
                                            style: "color: {cv}; --glow-color: {cv}",
                                            "{color}"
                                        }
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
                        }
                    }
                }

                div { class: "results-buttons", {children} }
            }
        }
    }
}
