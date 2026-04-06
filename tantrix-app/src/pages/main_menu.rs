use dioxus::prelude::*;

use crate::route::Route;

#[component]
pub fn MainMenu() -> Element {
    let nav = navigator();

    rsx! {
        div { class: "title-screen",

            div { class: "title-bg" }
            div { class: "title-glow" }

            div { class: "floating-hexes",
                div { class: "floating-hex hex-red",
                    div { class: "floating-hex-inner" }
                }
                div { class: "floating-hex hex-blue",
                    div { class: "floating-hex-inner" }
                }
                div { class: "floating-hex hex-yellow",
                    div { class: "floating-hex-inner" }
                }
                div { class: "floating-hex hex-green",
                    div { class: "floating-hex-inner" }
                }
            }

            div { class: "title-content",
                div { class: "title-logo-area",
                    div { class: "title-hex-frame" }
                    h1 { class: "title-logo", "TANTRIX" }
                    p { class: "title-subtitle", "Hexagonal Tile Strategy" }
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
