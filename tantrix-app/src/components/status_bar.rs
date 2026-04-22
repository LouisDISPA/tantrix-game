use dioxus::prelude::*;

use crate::hooks::{use_current_player, use_has_forced_move};
use crate::state::game_state::GameCtx;
use crate::state::online_state::{OnlineCtx, OnlinePhase};
use crate::state::ui_state::UiStateStoreExt;
use crate::svg::paths::color_css_var;

/// Floating HUD chip: player identity, phase instruction, bag count, and —
/// in online games — the room code and connection state.
#[component]
pub fn StatusBar() -> Element {
    let ctx = use_context::<GameCtx>();
    let ui = ctx.ui_state;

    let phase = ui.phase()();
    let message = ui.message()();
    let current_color = use_current_player()();
    let has_forced = use_has_forced_move()();
    let color_var = color_css_var(current_color);

    let bag_count = ctx.game_state.read().bag.count();

    let phase_text = phase.status_text(current_color, has_forced);
    let has_alert = !message.is_empty();

    let online = try_consume_context::<OnlineCtx>();
    let online_chunk = online.map(|oc| {
        let s = oc.state.read();
        let code = s.room_code.clone();
        let (label, dot_class) = match &s.phase {
            OnlinePhase::Connecting => ("Connecting…", "dot connecting"),
            OnlinePhase::Lobby { .. } => ("In lobby", "dot lobby"),
            OnlinePhase::Playing { .. } => ("Connected", "dot live"),
            OnlinePhase::Ended { .. } => ("Game over", "dot ended"),
            OnlinePhase::Failed { .. } => ("Disconnected", "dot failed"),
        };
        (code, label.to_string(), dot_class.to_string())
    });

    rsx! {
        div { class: "status-bar",
            div {
                class: "hud-chip",
                style: "--turn-color: {color_var}",

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

                if let Some((code, label, dot_class)) = online_chunk {
                    span { class: "hud-chip-sep", aria_hidden: "true", "·" }
                    div { class: "hud-chip-online",
                        span { class: "hud-chip-conn-dot {dot_class}" }
                        span { class: "hud-chip-conn-label", "{label}" }
                        span { class: "hud-chip-room", "{code}" }
                    }
                }
            }
        }
    }
}
