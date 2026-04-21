use dioxus::prelude::*;

use crate::hooks::{use_current_player, use_has_forced_move};
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};
use crate::svg::paths::color_css_var;

/// Top-of-board banner: current player, phase instructions, and counters.
#[component]
pub fn StatusBar() -> Element {
    let ctx = use_context::<GameCtx>();
    let ui = ctx.ui_state;

    let phase = ui.phase()();
    let message = ui.message()();
    let current_color = use_current_player()();
    let has_forced = use_has_forced_move()();
    let color_var = color_css_var(current_color);

    let (bag_count, board_count) = {
        let gs = ctx.game_state.read();
        (gs.bag.count(), gs.board.len())
    };

    let phase_text = phase.status_text(current_color, has_forced);
    let is_forced = has_forced && phase == TurnPhase::SelectTile;

    rsx! {
        div { class: "status-bar",
            div {
                display: "flex",
                align_items: "center",
                justify_content: "center",
                div {
                    class: "turn-banner",
                    class: if is_forced { "forced" },
                    style: "--turn-color: {color_var}",
                    span { class: "turn-banner-name", "{current_color}" }
                    span { class: "turn-banner-phase", "{phase_text}" }
                }
            }
            div { class: "status-pills",
                div { class: "status-item", "Bag: {bag_count}" }
                div { class: "status-item", "Board: {board_count}" }
                if !message.is_empty() {
                    div { class: "status-message", "{message}" }
                }
            }
        }
    }
}
