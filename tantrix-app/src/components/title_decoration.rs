use dioxus::prelude::*;

/// CSS class suffix per color — mapping stays in one place so adding a hex
/// is a single-line change.
const FLOATING_HEX_COLORS: &[&str] = &["red", "blue", "yellow", "green"];

/// Shared decorative background for title-style screens: gradient background,
/// glow overlay, and four floating color-coded hexagons. All animations live
/// in `menu.css` (`.floating-hex`, `.floating-hex-inner`).
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
