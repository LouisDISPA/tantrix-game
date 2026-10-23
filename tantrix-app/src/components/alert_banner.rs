//! Thin error/status line. Renders nothing when `message` is `None`.

use dioxus::prelude::*;

#[component]
pub fn AlertBanner(message: ReadSignal<Option<&'static str>>) -> Element {
    rsx! {
        if let Some(text) = message() {
            p { class: "surface surface-raised surface-blur banner alert-banner lobby-error", role: "alert", {text} }
        }
    }
}
