//! "Creating room…" / "Joining room…" splash shown while the matchbox
//! socket is being opened.

use dioxus::prelude::*;
use tantrix_net::SessionRole;

use crate::state::online_state::{OnlineState, OnlineStateStoreExt};

#[component]
pub fn ConnectingView(state: Store<OnlineState>) -> Element {
    let label = use_memo(move || match *state.role().read() {
        SessionRole::Host => "Creating room…",
        SessionRole::Client => "Joining room…",
    });

    rsx! {
        div { class: "title-screen",
            div { class: "title-content",
                h1 { class: "title-logo", "{label}" }
                p { class: "title-subtitle", "Room {state.room_code()}" }
            }
        }
    }
}
