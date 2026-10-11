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

use components::startup_loading::StartupLoading;
use route::Route;
use state::theme::THEME;

const TOKENS_CSS: Asset = asset!("/assets/tokens.css");
const COMPONENTS_CSS: Asset = asset!("/assets/components.css");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const BOARD_CSS: Asset = asset!("/assets/board.css");
const GAME_CSS: Asset = asset!("/assets/game.css");
const DESK_CSS: Asset = asset!("/assets/desk.css");
const HAND_CSS: Asset = asset!("/assets/hand.css");
const MENU_CSS: Asset = asset!("/assets/menu.css");
const LEARN_CSS: Asset = asset!("/assets/learn.css");
const ANIMATIONS_CSS: Asset = asset!("/assets/animations.css");
const LOADING_CSS: Asset = asset!("/assets/loading.css");
const FAVICON: Asset = asset!("/assets/favicon.svg");

pub fn app() -> Element {
    let theme = *THEME.resolve().read();
    // SSR and the first client render agree; mounted fires after hydration.
    let mut ready = use_signal(|| false);

    rsx! {
        document::Link { rel: "icon", href: FAVICON, sizes: "any", r#type: "image/svg+xml" }
        document::Stylesheet { href: MAIN_CSS }
        document::Stylesheet { href: TOKENS_CSS }
        document::Stylesheet { href: COMPONENTS_CSS }
        document::Stylesheet { href: BOARD_CSS }
        document::Stylesheet { href: HAND_CSS }
        document::Stylesheet { href: GAME_CSS }
        document::Stylesheet { href: DESK_CSS }
        document::Stylesheet { href: MENU_CSS }
        document::Stylesheet { href: LEARN_CSS }
        document::Stylesheet { href: ANIMATIONS_CSS }
        document::Stylesheet { href: LOADING_CSS }

        div {
            class: "app-root",
            "data-theme": "{theme.as_str()}",
            "data-app-ready": "{ready}",
            onmounted: move |_| ready.set(true),
            StartupLoading { ready: ready() }
            div {
                class: "app-content",
                inert: (!ready()).then_some(true),
                Router::<Route> {}
            }
        }
    }
}
