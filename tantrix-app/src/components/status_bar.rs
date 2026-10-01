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

/// Each value it shows is a memo (or the `message` field, read inline), so
/// it re-renders when its text changes: not on a hover, a drag, or an AI
/// selecting its tile.
#[component]
pub fn StatusBar() -> Element {
    #[cfg(test)]
    crate::util::render_count::record("StatusBar", ());
    let ctx = use_context::<GameCtx>();
    let ui = ctx.ui_state;

    let current_color = use_current_player();
    let has_forced = use_has_forced_move();
    let phase_text = use_memo(move || ui.phase()().status_text(current_color(), has_forced()));
    let bag_count = use_memo(move || ctx.game_state.read().bag_count());
    // The field's handle, not its value: each read below subscribes.
    let message = ui.message();

    rsx! {
        div { class: "status-bar",
            div {
                class: "hud-chip color-{color_slug(current_color())}",

                // Left — player identity
                div { class: "hud-chip-player",
                    div { class: "hud-chip-dot" }
                    span { class: "hud-chip-name", "{current_color}" }
                }

                span { class: "hud-chip-sep", aria_hidden: "true", "·" }

                // Center — phase instruction or alert
                span {
                    class: "hud-chip-phase",
                    class: if !message.read().is_empty() { "alert" },
                    if message.read().is_empty() { "{phase_text}" } else { "{message}" }
                }

                span { class: "hud-chip-sep", aria_hidden: "true", "·" }

                // Right — tiles remaining
                span { class: "hud-chip-bag", "{bag_count} in bag" }
            }
        }
    }
}
