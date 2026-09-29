//! In-game floating badge showing room code + connection state.
//! Sits next to the local-game `StatusBar` so that component stays pure.

use dioxus::prelude::*;

use crate::state::online_state::{queries, OnlinePhase, OnlineState, OnlineStateStoreExt};

#[component]
pub fn OnlineHudBadge(state: Store<OnlineState>) -> Element {
    let label_and_dot = use_memo(move || {
        let phase = state.phase();
        let phase = phase.read();
        match &*phase {
            OnlinePhase::Connecting => ("Connecting…", "dot connecting"),
            OnlinePhase::Lobby { .. } => ("In lobby", "dot lobby"),
            OnlinePhase::Playing { .. } => ("Connected", "dot live"),
            OnlinePhase::Ended { .. } => ("Game over", "dot ended"),
            OnlinePhase::Failed { .. } => ("Disconnected", "dot failed"),
        }
    });
    let spectating = use_memo(move || queries::is_spectator_of(state));

    rsx! {
        div { class: "online-hud-badge",
            span { class: "hud-chip-conn-dot {label_and_dot().1}" }
            span { class: "hud-chip-conn-label", "{label_and_dot().0}" }
            span { class: "hud-chip-room", "{state.room_code()}" }
            if spectating() {
                span { class: "online-spectator-badge", "Spectating" }
            }
        }
    }
}
