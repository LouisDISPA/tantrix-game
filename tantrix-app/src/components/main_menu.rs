use dioxus::prelude::*;

use crate::route::Route;
use crate::state::theme::{toggle_theme, THEME};

#[component]
pub fn MainMenu() -> Element {
    let nav = navigator();

    rsx! {
        div { class: "screen",
            button {
                class: "theme-toggle",
                onclick: move |_| toggle_theme(),
                "{THEME.read().icon()}"
            }
            div { class: "card",
                div { class: "menu-logo", "Tantrix" }
                div { class: "card-subtitle", "Hexagonal tile strategy game" }
                div { class: "menu-buttons",
                    button {
                        class: "btn btn-primary btn-large",
                        onclick: move |_| { let _ = nav.push(Route::PlayerSetup {}); },
                        "Local Game"
                    }
                    button {
                        class: "btn btn-disabled btn-large",
                        disabled: true,
                        "vs AI"
                        div { class: "coming-soon", "Coming soon" }
                    }
                    button {
                        class: "btn btn-disabled btn-large",
                        disabled: true,
                        "Online"
                        div { class: "coming-soon", "Coming soon" }
                    }
                }
            }
        }
    }
}
