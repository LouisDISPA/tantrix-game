//! Floating HUD chip: player identity, phase instruction, bag count.
//!
//! Online-specific info (room code + connection state) lives in a separate
//! `OnlineHudBadge` the online game view inserts alongside this bar — so
//! this component stays independent of `OnlineState`.

use dioxus::prelude::*;

use crate::hooks::{use_current_player, use_has_forced_move};
use crate::state::game_state::GameCtx;
use crate::state::ui_state::UiStateStoreExt;
use crate::svg::paths::color_name as color_slug;

#[component]
pub fn StatusBar() -> Element {
    let ctx = use_context::<GameCtx>();
    let ui = ctx.ui_state;

    let phase = ui.phase()();
    let message = ui.message()();
    let current_color = use_current_player()();
    let has_forced = use_has_forced_move()();
    let color_class = format!("color-{}", color_slug(current_color));

    let bag_count = ctx.game_state.read().bag.count();

    let phase_text = phase.status_text(current_color, has_forced);
    let has_alert = !message.is_empty();

    rsx! {
        div { class: "status-bar",
            div {
                class: "hud-chip {color_class}",

                // Left — player identity
                div { class: "hud-chip-player",
                    div { class: "hud-chip-dot" }
                    span { class: "hud-chip-name", "{current_color}" }
                }

                span { class: "hud-chip-sep", aria_hidden: "true", "·" }

                // Center — phase instruction or alert
                span {
                    class: "hud-chip-phase",
                    class: if has_alert { "alert" },
                    if has_alert { "{message}" } else { "{phase_text}" }
                }

                span { class: "hud-chip-sep", aria_hidden: "true", "·" }

                // Right — tiles remaining
                span { class: "hud-chip-bag", "{bag_count} in bag" }
            }
        }
    }
}
