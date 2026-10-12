use dioxus::prelude::*;

/// Server-rendered indicator, styled before the Wasm client starts.
#[component]
pub fn StartupLoading(ready: bool) -> Element {
    rsx! {
        div {
            id: "startup-loading",
            aria_hidden: ready,
            inert: ready.then_some(true),
            div { class: "loading-track", aria_hidden: true,
                div { class: "loading-ribbon" }
            }
            div { class: "loading-caption",
                span { class: "loading-hexes", aria_hidden: true,
                    span {}
                    span {}
                    span {}
                    span {}
                }
                span { id: "loading-status", role: "status", "Loading…" }
                // CSS reveals this after 15s, even if Wasm never starts.
                a { class: "loading-retry", href: "", "Reload" }
            }
        }
        noscript {
            style { "#startup-loading {{ display: none; }}" }
            p { class: "no-script", "Enable JavaScript to load Tantrix." }
        }
    }
}
