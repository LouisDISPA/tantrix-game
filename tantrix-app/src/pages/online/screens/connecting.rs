//! "Creating room…" / "Joining room…" splash shown while the matchbox
//! socket is being opened.
//!
//! Connecting normally takes about a second. If it drags on, the screen offers
//! a way out instead of leaving the player staring at a spinner: a handshake
//! can stall for reasons the app cannot see (an unresponsive STUN server, a
//! blocked UDP path), and starting over usually gets through.

use std::time::Duration;

use dioxus::prelude::*;
use tantrix_net::SessionRole;

use crate::state::online_state::{OnlineState, OnlineStateStoreExt};
use crate::util::sleep;

/// How long to show the plain splash before offering "Retry".
const SLOW_AFTER: Duration = Duration::from_secs(6);

#[component]
pub fn ConnectingView(state: Store<OnlineState>, on_retry: EventHandler<()>) -> Element {
    let label = use_memo(move || match *state.role().read() {
        SessionRole::Host => "Creating room…",
        SessionRole::Client => "Joining room…",
    });

    let mut slow = use_signal(|| false);
    use_hook(move || {
        spawn(async move {
            sleep(SLOW_AFTER).await;
            slow.set(true);
        });
    });

    rsx! {
        div { class: "title-screen",
            div { class: "title-content",
                h1 { class: "title-logo", "{label}" }
                p { class: "title-subtitle", "Room {state.room_code()}" }
                if slow() {
                    p { class: "lobby-waiting", "Still connecting… this is taking longer than usual." }
                    button {
                        class: "title-btn title-btn-primary",
                        onclick: move |_| on_retry.call(()),
                        div { class: "title-btn-inner", "Retry" }
                    }
                }
            }
        }
    }
}
