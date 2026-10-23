//! The online game's badge in the HUD row: room code + connection state.
//! `OnlineGameView` hands it to the shared game screens, so they stay
//! independent of `OnlineState`. It is only mounted while the room is playing
//! or the game is over: on any other phase the room shows another screen.

use dioxus::prelude::*;

use crate::state::online_state::{ConnectionStatus, OnlinePhase, OnlineState, OnlineStateStoreExt};

#[component]
pub fn OnlineHudBadge(state: Store<OnlineState>) -> Element {
    let label_and_dot = use_memo(move || {
        if matches!(&*state.phase().read(), OnlinePhase::Ended { .. }) {
            ("Game over", "dot ended")
        } else {
            match *state.connection().read() {
                ConnectionStatus::Connected => ("Connected", "dot live"),
                ConnectionStatus::Connecting | ConnectionStatus::Reconnecting { .. } => {
                    ("Reconnecting", "dot")
                }
                ConnectionStatus::Stopped | ConnectionStatus::OpenedElsewhere => {
                    ("Disconnected", "dot")
                }
            }
        }
    });
    rsx! {
        div { class: "surface surface-blur online-hud-badge", "aria-label": label_and_dot().0,
            span { class: "hud-chip-conn-dot {label_and_dot().1}" }
            span { class: "hud-chip-conn-label", "{label_and_dot().0}" }
            span { class: "badge badge-code hud-chip-room", "{state.room_code()}" }
        }
    }
}

/// Persistent room feedback. Uses the same compact card as turn announcements.
#[component]
pub fn ConnectionNotice(state: Store<OnlineState>, on_retry: EventHandler<()>) -> Element {
    let status = state.connection();
    rsx! {
        if !status.read().is_connected() || *state.seat_changed().read() {
            div { class: "surface surface-raised surface-blur banner connection-notice",
                div { class: "banner-copy", role: "status", "aria-live": "polite", "aria-atomic": "true",
                    strong { class: "banner-title",
                        match *status.read() {
                            ConnectionStatus::Connecting => "Reconnecting…".to_string(),
                            ConnectionStatus::Reconnecting { attempt } => format!("Reconnecting… Attempt {attempt} of 3"),
                            ConnectionStatus::Stopped => "Couldn't reconnect".to_string(),
                            ConnectionStatus::OpenedElsewhere => "This room is active in another tab".to_string(),
                            ConnectionStatus::Connected => "Your seat changed".to_string(),
                        }
                    }
                    span { class: "banner-detail",
                        if status.read().is_connected() {
                            "You're watching now. Another player has your seat."
                        } else {
                            "Last board shown. Play is paused."
                        }
                    }
                }
                div { class: "banner-actions",
                    if matches!(*status.read(), ConnectionStatus::Stopped | ConnectionStatus::OpenedElsewhere) {
                        button { class: "button button-small button-primary", onclick: move |_| on_retry.call(()),
                            if *status.read() == ConnectionStatus::OpenedElsewhere { "Resume here" } else { "Retry" }
                        }
                    }
                    if status.read().is_connected() {
                        button { class: "button button-small button-ghost", onclick: move |_| state.seat_changed().set(false), "Dismiss" }
                    } else {
                        button { class: "button button-small button-ghost", onclick: move |_| { let _ = navigator().push(crate::route::Route::OnlineEntry {}); }, "Back to online menu" }
                    }
                }
            }
        }
    }
}
