//! The game screen's menu: a `⋯` button that opens a popover of whatever the
//! screen puts in it. A wide window shows the same items in a row instead
//! (the stylesheet decides), so the menu is the same on every screen.

use dioxus::prelude::*;

/// Any click on an item closes the popover (an item that wants it to stay,
/// like a zoom step, stops the click); so does a click anywhere else, and
/// Escape.
#[component]
pub fn HudMenu(children: Element) -> Element {
    let mut open = use_signal(|| false);

    rsx! {
        div {
            class: "hud-menu",
            class: if open() { "open" },
            onkeydown: move |e| {
                if open() && e.key() == Key::Escape {
                    // Closing the menu is all Escape does: not a step back.
                    e.stop_propagation();
                    open.set(false);
                }
            },
            if open() {
                div { class: "hud-menu-backdrop", onclick: move |_| open.set(false) }
            }
            button {
                class: "hud-menu-toggle",
                aria_label: "Menu",
                aria_haspopup: "true",
                aria_expanded: "{open()}",
                onclick: move |_| open.set(!open()),
                "⋯"
            }
            div { class: "hud-menu-items", onclick: move |_| open.set(false), {children} }
        }
    }
}
