//! Shared setup card, with room for each page's own controls.

use std::time::Duration;

use dioxus::prelude::*;

use crate::util::sleep;

const BACK_EXIT: Duration = Duration::from_millis(180);

#[component]
pub fn MenuCard(
    #[props(into)] title: String,
    #[props(into)] subtitle: String,
    #[props(default)] wide: bool,
    back: Option<EventHandler<()>>,
    children: Element,
) -> Element {
    let mut leaving = use_signal(|| false);

    rsx! {
        div {
            class: "surface surface-raised surface-backdrop menu-card",
            class: if wide { "menu-card-wide" },
            class: if leaving() { "menu-card-exit" },
            if let Some(back) = back {
                button {
                    class: "button button-ghost menu-back-btn",
                    disabled: leaving(),
                    onclick: move |_| async move {
                        if *leaving.peek() {
                            return;
                        }
                        // Keep this card mounted until its exit animation finishes.
                        leaving.set(true);
                        sleep(BACK_EXIT).await;
                        back.call(());
                    },
                    "← Back"
                }
            }
            h1 { class: "brand-heading brand-heading-animated menu-card-title", {title} }
            p { class: "menu-card-subtitle", {subtitle} }
            {children}
        }
    }
}
