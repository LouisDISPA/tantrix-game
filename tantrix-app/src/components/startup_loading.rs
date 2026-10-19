use dioxus::prelude::*;

const NO_SCRIPT_FALLBACK: &str = concat!(
    "<style>#startup-loading { display: none; }</style>",
    r#"<p class="no-script">Enable JavaScript to load Tantrix.</p>"#,
);

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
        // Browsers parse noscript's contents as text when scripting is enabled.
        // Keep the static fallback opaque to the markerless hydration walk.
        noscript {
            dangerous_inner_html: NO_SCRIPT_FALLBACK,
        }
    }
}
