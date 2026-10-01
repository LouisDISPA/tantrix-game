use dioxus::prelude::*;
use tantrix_game::Color;

use crate::state::game_state::GameCtx;
use crate::state::ui_state::{Hovered, UiStateStoreExt};
use crate::svg::paths::color_name as color_slug;

/// A single row in the scores list: the player color with its score and
/// winner flag, which [`ScoresPanel`] works out once for every row. The
/// active (click-locked) flag and click/hover dispatch are derived from
/// `GameCtx`.
#[component]
fn ScoreRow(color: Color, score: usize, is_winner: bool) -> Element {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    let is_active = use_memo(move || ui_state.highlighted_color()() == Some(color));

    let color_class = format!("color-{}", color_slug(color));

    rsx! {
        div {
            class: "results-row {color_class}",
            class: if is_winner { "results-row-winner" },
            class: if is_active() { "results-row-active" },
            role: "button",
            tabindex: "0",
            aria_label: "Highlight score path",
            onclick: move |_| {
                let current = *ui_state.highlighted_color().read();
                if current == Some(color) {
                    ui_state.highlighted_color().set(None);
                } else {
                    ui_state.highlighted_color().set(Some(color));
                }
            },
            onkeydown: move |e| {
                if matches!(e.key(), Key::Enter)
                    || matches!(e.key(), Key::Character(c) if c == " ")
                {
                    let current = *ui_state.highlighted_color().read();
                    if current == Some(color) {
                        ui_state.highlighted_color().set(None);
                    } else {
                        ui_state.highlighted_color().set(Some(color));
                    }
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
                class: if is_winner { "winner" },
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

    // Score the board once for the whole card: each row's points, and the
    // winners among them.
    let standings = use_memo(move || {
        let gs = ctx.game_state.read();
        let scores = gs.scores();
        let rows: Vec<(Color, usize)> = gs
            .playing_colors()
            .iter()
            .map(|&color| (color, scores[color].score))
            .collect();
        (rows, gs.winners_from(&scores))
    });
    let standings = standings.read();
    let (rows, winners) = &*standings;
    let is_tie = winners.len() > 1;

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
                                    let color_class = format!("color-{}", color_slug(color));
                                    rsx! {
                                        span {
                                            key: "{color:?}",
                                            if i > 0 {
                                                span { class: "results-tie-separator", " & " }
                                            }
                                            span {
                                                class: "results-winner-name {color_class}",
                                                "{color}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        {
                            let winner_color = winners[0];
                            let winner_class = format!("color-{}", color_slug(winner_color));
                            rsx! {
                                div { class: "results-winner-label", "Winner" }
                                div {
                                    class: "results-winner-name {winner_class}",
                                    "{winner_color}"
                                }
                            }
                        }
                    }
                }

                div { class: "results-divider" }

                div { class: "results-stats",
                    for &(c , score) in rows.iter() {
                        ScoreRow {
                            key: "{c:?}",
                            color: c,
                            score,
                            is_winner: winners.contains(&c),
                        }
                    }
                }

                div { class: "results-buttons", {children} }
            }
        }
    }
}
