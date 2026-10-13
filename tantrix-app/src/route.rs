#![allow(unreachable_code)] // The generated router contains unreachable fallback branches.

use dioxus::prelude::*;

use crate::components::theme_toggle::ThemeToggle;
use crate::components::title_decoration::TitleDecoration;
use crate::pages::about::About;
#[cfg(not(any(feature = "desktop", feature = "native")))]
use crate::pages::download::Download;
use crate::pages::learn::Learn;
use crate::pages::local_game::LocalGame;
use crate::pages::main_menu::MainMenu;
use crate::pages::not_found::NotFound;
use crate::pages::online::{JoinRoom, OnlineEntry, OnlineRoom};

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    #[layout(AppLayout)]
    #[route("/")]
    MainMenu {},
    #[route("/learn")]
    Learn {},
    #[route("/about")]
    About {},
    #[cfg(not(any(feature = "desktop", feature = "native")))]
    #[route("/download")]
    Download {},
    #[route("/game?:seed")]
    LocalGame { seed: Option<u64> },
    #[route("/online")]
    OnlineEntry {},
    #[route("/online/:code")]
    OnlineRoom { code: String },
    #[route("/join/:code")]
    JoinRoom { code: String },
    #[route("/:..path")]
    NotFound { path: Vec<String> },
}

#[derive(Clone, Copy)]
pub struct MenuVisit {
    pub has_seen_main: Signal<bool>,
}

#[component]
fn AppLayout() -> Element {
    let has_seen_main = use_signal(|| false);
    use_context_provider(move || MenuVisit { has_seen_main });

    rsx! {
        div { class: "menu-background", aria_hidden: "true", TitleDecoration {} }
        div { class: "app-page", Outlet::<Route> {} }
        ThemeToggle { class: "theme-toggle" }
    }
}
