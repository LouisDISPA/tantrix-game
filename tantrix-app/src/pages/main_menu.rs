use dioxus::prelude::*;

use crate::components::title_decoration::TitleDecoration;
use crate::route::Route;

#[component]
pub fn MainMenu() -> Element {
    let nav = navigator();

    rsx! {
        div { class: "title-screen",
            TitleDecoration {}

            div { class: "title-content",
                div { class: "title-logo-area",
                    div { class: "title-hex-frame" }
                    h1 { class: "title-logo", "TANTRIX" }
                    p { class: "title-subtitle", "Hexagonal Strategy Game" }
                }

                div { class: "title-buttons",
                    button {
                        class: "title-btn title-btn-primary",
                        onclick: move |_| {
                            let _ = nav.push(Route::LocalGame {});
                        },
                        div { class: "title-btn-inner", "Local Game" }
                    }
                    button { class: "title-btn title-btn-disabled", disabled: true,
                        div { class: "title-btn-inner",
                            "Online"
                            span { class: "title-coming-soon", "Coming soon" }
                        }
                    }
                }
            }
        }
    }
}
