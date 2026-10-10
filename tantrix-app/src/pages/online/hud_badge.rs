//! The online game's badge in the HUD row: room code + connection state.
//! `OnlineGameView` hands it to the shared game screens, so they stay
//! independent of `OnlineState`. It is only mounted while the room is playing
//! or the game is over: on any other phase the room shows another screen.

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
        div { class: "surface surface-blur online-hud-badge",
            span { class: "hud-chip-conn-dot {label_and_dot().1}" }
            span { class: "hud-chip-conn-label", "{label_and_dot().0}" }
            span { class: "badge badge-code hud-chip-room", "{state.room_code()}" }
        }
    }
}
