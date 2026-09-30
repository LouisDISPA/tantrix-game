use dioxus::prelude::*;

use crate::components::title_decoration::TitleDecoration;
use crate::pages::local_game::LocalGame;
use crate::pages::main_menu::MainMenu;
use crate::pages::online::{JoinRoom, OnlineEntry, OnlineRoom};
use crate::state::theme::{toggle_theme, THEME};

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    #[layout(AppLayout)]
    #[route("/")]
    MainMenu {},
    #[route("/game?:seed")]
    LocalGame { seed: Option<u64> },
    #[route("/online")]
    OnlineEntry {},
    #[route("/online/:code")]
    OnlineRoom { code: String },
    #[route("/join/:code")]
    JoinRoom { code: String },
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
        button { class: "theme-toggle", onclick: move |_| toggle_theme(), "{THEME.resolve().read().icon()}" }
    }
}
