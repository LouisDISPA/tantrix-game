//! Shared primary action for menu and setup pages.

use dioxus::prelude::*;

#[component]
pub fn MenuButton(
    on_click: EventHandler<()>,
    #[props(default)] disabled: bool,
    #[props(default)] full_width: bool,
    children: Element,
) -> Element {
    rsx! {
        button {
            class: "title-btn title-btn-primary",
            class: if full_width { "menu-button-full" },
            disabled,
            onclick: move |_| on_click.call(()),
            div { class: "title-btn-inner", {children} }
        }
    }
}
