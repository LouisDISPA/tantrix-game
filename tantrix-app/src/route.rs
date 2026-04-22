use std::fmt;
use std::str::FromStr;

use dioxus::prelude::*;
use tantrix_net::SessionRole;

use crate::pages::local_game::LocalGame;
use crate::pages::main_menu::MainMenu;
use crate::pages::online::{JoinRoom, OnlineEntry, OnlineRoom};
use crate::state::theme::{toggle_theme, THEME};

/// Role component of the `/online/:code/:role/:nickname` route.
///
/// A dedicated newtype exists so the router rejects malformed URLs at parse
/// time instead of silently falling back to `Client` on invalid input.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RoleParam {
    Host,
    Client,
}

impl RoleParam {
    pub fn as_session_role(self) -> SessionRole {
        match self {
            RoleParam::Host => SessionRole::Host,
            RoleParam::Client => SessionRole::Client,
        }
    }
}

impl fmt::Display for RoleParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RoleParam::Host => f.write_str("host"),
            RoleParam::Client => f.write_str("client"),
        }
    }
}

impl FromStr for RoleParam {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "host" => Ok(RoleParam::Host),
            "client" => Ok(RoleParam::Client),
            _ => Err("expected 'host' or 'client'"),
        }
    }
}

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
        role: RoleParam,
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
