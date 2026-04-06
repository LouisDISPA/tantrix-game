use dioxus::prelude::*;

use crate::pages::{local_game::LocalGame, main_menu::MainMenu};
use crate::state::theme::{toggle_theme, THEME};

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    #[layout(AppLayout)]
    #[route("/")]
    MainMenu {},
    #[route("/game")]
    LocalGame {},
}

#[component]
fn AppLayout() -> Element {
    rsx! {
        Outlet::<Route> {}
        button {
            class: "theme-toggle",
            onclick: move |_| toggle_theme(),
            "{THEME.read().icon()}"
        }
    }
}
