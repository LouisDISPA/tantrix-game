//! Shared content area for menu and setup pages.

use dioxus::prelude::*;

#[component]
pub fn MenuScreen(
    #[props(default)] scroll: bool,
    #[props(default)] quick: bool,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: "title-screen",
            class: if scroll { "menu-screen-scroll" },
            class: if quick { "menu-screen-quick" },
            div {
                class: "title-content stack items-center justify-center",
                class: if scroll { "menu-content-scroll" },
                {children}
            }
        }
    }
}
