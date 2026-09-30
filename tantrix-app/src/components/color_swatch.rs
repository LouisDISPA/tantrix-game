//! Color hex shared by local player rows and online seat choices.

use dioxus::prelude::*;
use tantrix_game::Color;

use crate::svg::paths::color_name as color_slug;

#[component]
pub fn ColorSwatch(
    color: Color,
    #[props(default)] large: bool,
    #[props(default)] selected: bool,
) -> Element {
    rsx! {
        div {
            class: "color-swatch color-{color_slug(color)}",
            class: if large { "color-swatch-large" },
            aria_hidden: "true",
            if large {
                div { class: "color-swatch-inner" }
            }
            if selected {
                span { class: "color-swatch-check", "✓" }
            }
        }
    }
}
