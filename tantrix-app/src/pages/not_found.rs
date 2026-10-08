use dioxus::prelude::*;

use crate::components::menu_button::MenuButton;
use crate::components::menu_card::MenuCard;
use crate::components::menu_screen::MenuScreen;
use crate::route::Route;

#[component]
pub fn NotFound(path: Vec<String>) -> Element {
    let _ = path;
    #[cfg(any(feature = "server", feature = "worker"))]
    dioxus_fullstack_core::FullstackContext::commit_http_status(http::StatusCode::NOT_FOUND, None);
    let nav = navigator();

    rsx! {
        MenuScreen {
            MenuCard {
                title: "Page not found",
                subtitle: "There's no page at this address.",
                MenuButton {
                    on_click: move |_| { let _ = nav.push(Route::MainMenu {}); },
                    "Go to main menu"
                }
            }
        }
    }
}
