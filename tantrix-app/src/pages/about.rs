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
                subtitle: "Tantrix",
                back: move |_| {
                    let _ = nav.push(Route::MainMenu {});
                },
                main { class: "about-copy stack",
                    p {
                        "This application and website is a side project by Louis Dispa. It is not official and not affiliated with Tantrix."
                    }
                    p { "Developed with the help of AI tools such as codex and claude." }
                    p { "All the code is open source and available on github." }
                    a {
                        class: "button",
                        style: "text-decoration: none",
                        href: "https://github.com/LouisDISPA/tantrix-game",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "View on GitHub"
                    }
                }
            }
        }
    }
}
