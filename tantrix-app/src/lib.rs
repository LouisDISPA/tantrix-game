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

// SSR embeds the CSS in the head. Standalone clients also need the contents;
// fullstack browser clients reuse the server's style tag during hydration.
// The Worker renders on wasm32 too, so gate by features rather than target.
#[cfg(any(feature = "server", feature = "worker", not(feature = "fullstack")))]
const APP_CSS: &str = concat!(
    include_str!("../assets/main.css"),
    "\n",
    include_str!("../assets/tokens.css"),
    "\n",
    include_str!("../assets/components.css"),
    "\n",
    include_str!("../assets/board.css"),
    "\n",
    include_str!("../assets/hand.css"),
    "\n",
    include_str!("../assets/game.css"),
    "\n",
    include_str!("../assets/desk.css"),
    "\n",
    include_str!("../assets/menu.css"),
    "\n",
    include_str!("../assets/learn.css"),
    "\n",
    include_str!("../assets/animations.css"),
    "\n",
    include_str!("../assets/loading.css"),
);

#[cfg(all(
    feature = "fullstack",
    not(any(feature = "server", feature = "worker"))
))]
const APP_CSS: &str = "";

const FAVICON: Asset = asset!("/assets/favicon.svg");

pub fn app() -> Element {
    let theme = *THEME.resolve().read();
    // SSR and the first client render agree; mounted fires after hydration.
    let mut ready = use_signal(|| false);

    rsx! {
        document::Link {
            rel: "icon",
            href: FAVICON,
            sizes: "any",
            r#type: "image/svg+xml",
        }
        document::Link {
            rel: "manifest",
            href: "/manifest.webmanifest",
        }
        document::Link {
            rel: "apple-touch-icon",
            href: "/icons/apple-touch-icon.png",
        }
        document::Meta {
            name: "theme-color",
            content: "#171717",
        }
        document::Meta {
            name: "apple-mobile-web-app-capable",
            content: "yes",
        }
        document::Meta {
            name: "apple-mobile-web-app-title",
            content: "Tantrix",
        }
        document::Style { id: "app-styles", {APP_CSS} }

        div {
            class: "app-root",
            "data-theme": "{theme.as_str()}",
            "data-app-ready": "{ready}",
            onmounted: move |_| ready.set(true),
            StartupLoading { ready: ready() }
            div { class: "app-content", inert: (!ready()).then_some(true), Router::<Route> {} }
        }
    }
}
