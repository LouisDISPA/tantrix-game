//! The big monospaced `ABC123` badge shown at the top of the lobby.

use dioxus::prelude::*;

#[component]
pub fn RoomCodeBadge(code: ReadSignal<String>) -> Element {
    rsx! { div { class: "badge badge-code lobby-room-code", "{code}" } }
}
