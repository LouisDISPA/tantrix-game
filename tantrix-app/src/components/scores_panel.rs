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
                if matches!(e.key(), Key::Named(NamedKey::Enter))
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
            // Only a mouse hovers: a finger's "hover" would stick after the tap.
            onpointerenter: move |e| {
                if e.data().pointer_type() == "mouse" {
                    ui_state.hovered().set(Hovered::ScoringColor(color));
                }
            },
            onpointerleave: move |e| {
                if e.data().pointer_type() == "mouse" {
                    ui_state.hovered().set(Hovered::None);
                }
            },
            div { class: "results-player",
                div { class: "color-swatch results-color-swatch" }
                span { class: "results-name", "{color}" }
            }
            span { class: "results-points", class: if is_winner { "winner" }, "{score}" }
        }
    }
}

/// End-of-game scores card: winner header, per-player rows, and a slot for
/// the parent screen to inject its own action buttons (Play Again, Main
/// Menu). It takes the panel's place on the game screen, under the board
/// (beside it on a wide window).
#[component]
pub fn ScoresPanel(children: Element) -> Element {
    let ctx = use_context::<GameCtx>();

    // Score the board once for the whole card, then derive each row's
    // points and the winners among them from those scores.
    let scores = use_memo(move || ctx.game_state.read().scores());
    let rows = use_memo(move || {
        let scores = scores.read();
        ctx.game_state
            .read()
            .playing_colors()
            .iter()
            .map(|&color| (color, scores[color].score))
            .collect::<Vec<(Color, usize)>>()
    });
    let winners = use_memo(move || ctx.game_state.read().winners_from(&scores.read()));

    rsx! {
        section { class: "surface surface-raised surface-blur results-panel",
            div { class: "surface surface-raised surface-blur board-results-card",
                div { class: "results-header",
                    if winners.read().len() > 1 {
                        div { class: "results-winner-label", "Tie" }
                        div { class: "results-tie-names",
                            for (i, color) in winners.read().iter().copied().enumerate() {
                                span { key: "{color:?}",
                                    if i > 0 {
                                        span { class: "results-tie-separator", " & " }
                                    }
                                    span { class: format!("results-winner-name color-{}", color_slug(color)),
                                        "{color}"
                                    }
                                }
                            }
                        }
                    } else {
                        {
                            let winner_color = winners.read()[0];
                            let winner_class = format!("color-{}", color_slug(winner_color));
                            rsx! {
                                div { class: "results-winner-label", "Winner" }
                                div { class: "results-winner-name {winner_class}", "{winner_color}" }
                            }
                        }
                    }
                }

                div { class: "results-divider" }

                div { class: "results-stats",
                    for (c, score) in rows.read().iter().copied() {
                        ScoreRow {
                            key: "{c:?}",
                            color: c,
                            score,
                            is_winner: winners.read().contains(&c),
                        }
                    }
                }

                div { class: "results-buttons", {children} }
            }
        }
    }
}
