use dioxus::prelude::*;

use crate::components::menu_card::MenuCard;
use crate::components::menu_screen::MenuScreen;
use crate::route::Route;

#[component]
pub fn About() -> Element {
    let nav = navigator();

    rsx! {
        MenuScreen { scroll: true,
            MenuCard {
                title: "About",
                subtitle: "A hexagonal strategy game",
                back: move |_| {
                    let _ = nav.push(Route::MainMenu {});
                },
                main { class: "about-copy",
                    section {
                        h2 { "What is this app?" }
                        p {
                            "Play Tantrix locally or in an online room. Place hexagonal tiles, connect your color, and build the longest line or loop."
                        }
                    }
                    section {
                        h2 { "An independent project" }
                        p {
                            "This is an unofficial implementation. It is not affiliated with the official Tantrix game."
                        }
                    }
                }
            }
        }
    }
}
