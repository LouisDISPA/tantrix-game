use dioxus::prelude::*;

/// Shared decorative background for title-style screens:
/// gradient background, glow effect, and floating hex animations.
#[component]
pub fn TitleDecoration() -> Element {
    rsx! {
        div { class: "title-bg" }
        div { class: "title-glow" }
        div { class: "floating-hexes",
            div { class: "floating-hex hex-red",
                div { class: "floating-hex-inner" }
            }
            div { class: "floating-hex hex-blue",
                div { class: "floating-hex-inner" }
            }
            div { class: "floating-hex hex-yellow",
                div { class: "floating-hex-inner" }
            }
            div { class: "floating-hex hex-green",
                div { class: "floating-hex-inner" }
            }
        }
    }
}
