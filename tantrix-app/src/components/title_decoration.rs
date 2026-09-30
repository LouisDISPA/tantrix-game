use dioxus::prelude::*;

/// CSS class suffix per color — mapping stays in one place so adding a hex
/// is a single-line change.
const FLOATING_HEX_COLORS: &[&str] = &["red", "blue", "yellow", "green"];

/// Persistent decorative background for menu screens, mounted by the router
/// layout so navigating between menus does not restart its animations.
#[component]
pub fn TitleDecoration() -> Element {
    rsx! {
        div { class: "title-bg" }
        div { class: "title-glow" }
        div { class: "floating-hexes",
            for color in FLOATING_HEX_COLORS.iter() {
                div {
                    key: "{color}",
                    class: "floating-hex hex-{color}",
                    div { class: "floating-hex-inner" }
                }
            }
        }
    }
}
