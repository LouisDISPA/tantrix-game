use dioxus::prelude::*;
use tantrix_game::Color;

use crate::state::game_state::{determine_winners, GameCtx};
use crate::state::ui_state::{Hovered, UiStateStoreExt};
use crate::svg::paths::color_css_var;

/// A single row in the scores list. Takes only the player color — the score,
/// winner flag, active (click-locked) flag, and click/hover dispatch are all
/// derived from `GameCtx`.
#[component]
fn ScoreRow(color: Color) -> Element {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    let score = use_memo(move || ctx.game_state.read().board.scores()[color].score);
    let is_winner = use_memo(move || {
        let gs = ctx.game_state.read();
        let active = gs.turn_tracker.playing_colors().to_vec();
        let scores = gs.board.scores();
        let (winners, _) = determine_winners(&active, &scores);
        winners.contains(&color)
    });
    let is_active = use_memo(move || ui_state.highlighted_color()() == Some(color));

    let cv = color_css_var(color);

    rsx! {
        div {
            class: "results-row",
            class: if is_winner() { "results-row-winner" },
            class: if is_active() { "results-row-active" },
            style: "--player-color: {cv}",
            onclick: move |_| {
                let current = *ui_state.highlighted_color().read();
                if current == Some(color) {
                    ui_state.highlighted_color().set(None);
                } else {
                    ui_state.highlighted_color().set(Some(color));
                }
            },
            onmouseenter: move |_| ui_state.hovered().set(Hovered::ScoringColor(color)),
            onmouseleave: move |_| ui_state.hovered().set(Hovered::None),
            div { class: "results-player",
                div { class: "results-color-swatch" }
                span { class: "results-name", "{color}" }
            }
            span {
                class: "results-points",
                class: if is_winner() { "winner" },
                "{score}"
            }
        }
    }
}

/// End-of-game scores card: winner header, per-player rows, and a slot for
/// the parent screen to inject its own action buttons (Play Again, Main Menu).
#[component]
pub fn ScoresPanel(children: Element) -> Element {
    let ctx = use_context::<GameCtx>();

    let (active_players, winners, is_tie) = {
        let gs = ctx.game_state.read();
        let active: Vec<Color> = gs.turn_tracker.playing_colors().to_vec();
        let scores = gs.board.scores();
        let (winners, is_tie) = determine_winners(&active, &scores);
        (active, winners, is_tie)
    };

    rsx! {
        div { class: "board-results-overlay",
            div { class: "board-results-card",
                div { class: "hex-frame results-hex-frame" }

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
                                            style: "--player-color: {cv}",
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
                                    style: "--player-color: {winner_var}",
                                    "{winner_color}"
                                }
                            }
                        }
                    }
                }

                div { class: "results-divider" }

                div { class: "results-stats",
                    for &c in active_players.iter() {
                        ScoreRow { key: "{c:?}", color: c }
                    }
                }

                div { class: "results-buttons", {children} }
            }
        }
    }
}
