//! In-game floating badge showing room code + connection state.
//! Sits next to the local-game `StatusBar` so that component stays pure.
//! Only `OnlineGameView` mounts it, so the room is playing or the game is
//! over: on any other phase the room shows another screen.

use dioxus::prelude::*;

use crate::state::online_state::{OnlinePhase, OnlineState, OnlineStateStoreExt};

#[component]
pub fn OnlineHudBadge(state: Store<OnlineState>) -> Element {
    let label_and_dot = use_memo(move || {
        if matches!(&*state.phase().read(), OnlinePhase::Ended { .. }) {
            ("Game over", "dot ended")
        } else {
            ("Connected", "dot live")
        }
    });
    rsx! {
        div { class: "online-hud-badge",
            span { class: "hud-chip-conn-dot {label_and_dot().1}" }
            span { class: "hud-chip-conn-label", "{label_and_dot().0}" }
            span { class: "hud-chip-room", "{state.room_code()}" }
        }
    }
}
