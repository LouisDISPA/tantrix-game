use dioxus::prelude::*;

use crate::components::menu_button::MenuButton;
use crate::components::menu_screen::MenuScreen;
use crate::route::{MenuVisit, Route};

#[component]
pub fn MainMenu() -> Element {
    let nav = navigator();
    let mut visit = use_context::<MenuVisit>();
    // The router layout survives menu navigation, so only the first visit
    // uses the longer title entrance.
    let quick = use_hook(move || *visit.has_seen_main.peek());
    use_effect(move || visit.has_seen_main.set(true));

    rsx! {
        MenuScreen { quick,
                div { class: "title-logo-area",
                    div { class: "hex-frame title-hex-frame" }
                    h1 { class: "title-logo", "TANTRIX" }
                    p { class: "title-subtitle", "Hexagonal Strategy Game" }
                }

                div { class: "title-buttons",
                    MenuButton {
                        on_click: move |_| {
                            let _ = nav.push(Route::LocalGame { seed: None });
                        },
                        "Local Game"
                    }
                    MenuButton {
                        on_click: move |_| {
                            let _ = nav.push(Route::OnlineEntry {});
                        },
                        "Online"
                    }
                    div { class: "title-secondary-links",
                        button {
                            class: "title-btn-link",
                            onclick: move |_| { let _ = nav.push(Route::Learn {}); },
                            "Learn the rules · 2 min →"
                        }
                        button {
                            class: "title-btn-link",
                            onclick: move |_| { let _ = nav.push(Route::About {}); },
                            "About"
                        }
                    }
                }
        }
    }
}
