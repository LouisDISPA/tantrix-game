//! The Tantrix Dioxus app as a library, so it can be rendered by more than one
//! host: the `tantrix-app` binary (web client / native fullstack server) and
//! the `tantrix-worker` crate (server-side rendering on Cloudflare Workers).

use dioxus::prelude::*;

mod components;
mod hooks;
mod pages;
mod route;
mod state;
mod svg;
mod util;

#[cfg(feature = "server")]
pub mod server;

pub use util::init_logging;

use route::Route;
use state::theme::THEME;

const MAIN_CSS: Asset = asset!("/assets/main.css");
const BOARD_CSS: Asset = asset!("/assets/board.css");
const HAND_CSS: Asset = asset!("/assets/hand.css");
const MENU_CSS: Asset = asset!("/assets/menu.css");
const ANIMATIONS_CSS: Asset = asset!("/assets/animations.css");

pub fn app() -> Element {
    let theme = *THEME.resolve().read();

    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        document::Stylesheet { href: BOARD_CSS }
        document::Stylesheet { href: HAND_CSS }
        document::Stylesheet { href: MENU_CSS }
        document::Stylesheet { href: ANIMATIONS_CSS }

        div { class: "app-root", "data-theme": "{theme.as_str()}", Router::<Route> {} }
    }
}
