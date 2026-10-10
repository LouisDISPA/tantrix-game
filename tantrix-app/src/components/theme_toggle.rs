//! The light/dark switch: a round button in the corner of the menu screens,
//! a row of the game's menu.

use dioxus::prelude::*;

use crate::state::theme::{toggle_theme, Theme, THEME};

/// `class` styles the button; `labelled` adds the name of the theme it
/// switches to after the icon.
#[component]
pub fn ThemeToggle(
    #[props(into, default)] class: String,
    #[props(default)] labelled: bool,
) -> Element {
    let theme = *THEME.resolve().read();
    let (action, label) = match theme {
        Theme::Dark => ("Switch to light theme", "Light theme"),
        Theme::Light => ("Switch to dark theme", "Dark theme"),
    };

    rsx! {
        button {
            class: "button button-ghost",
            class: if !labelled { "button-icon" },
            class: "{class}",
            aria_label: action,
            onclick: move |_| toggle_theme(),
            span { aria_hidden: "true", "{theme.icon()}" }
            if labelled {
                span { class: "theme-toggle-label", {label} }
            }
        }
    }
}
