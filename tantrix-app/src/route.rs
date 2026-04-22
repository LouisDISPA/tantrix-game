use dioxus::prelude::*;

use crate::pages::local_game::LocalGame;
use crate::pages::main_menu::MainMenu;
use crate::pages::online::{JoinRoom, OnlineEntry, OnlineRoom};
use crate::state::theme::{toggle_theme, THEME};

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    #[layout(AppLayout)]
    #[route("/")]
    MainMenu {},
    #[route("/game")]
    LocalGame {},
    #[route("/online")]
    OnlineEntry {},
    #[route("/online/:code/:role/:nickname")]
    OnlineRoom {
        code: String,
        role: String,
        nickname: String,
    },
    #[route("/join/:code")]
    JoinRoom { code: String },
}

#[component]
fn AppLayout() -> Element {
    rsx! {
        Outlet::<Route> {}
        button { class: "theme-toggle", onclick: move |_| toggle_theme(), "{THEME.resolve().read().icon()}" }
    }
}
