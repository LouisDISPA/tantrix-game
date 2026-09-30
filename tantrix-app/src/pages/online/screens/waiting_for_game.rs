//! Shown inside the online game route while we've transitioned past the
//! lobby but haven't received the first `GameStarted` snapshot yet.

use dioxus::prelude::*;

#[component]
pub fn WaitingForGame() -> Element {
    rsx! {
        div { class: "title-screen",
            div { class: "title-content",
                p { class: "title-subtitle", "Waiting for game…" }
            }
        }
    }
}
