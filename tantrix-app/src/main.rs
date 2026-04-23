use dioxus::prelude::*;

mod components;
mod hooks;
mod pages;
mod route;
mod state;
mod svg;
mod util;

#[cfg(feature = "server")]
mod server;

use route::Route;
use state::theme::THEME;

const MAIN_CSS: Asset = asset!("/assets/main.css");
const BOARD_CSS: Asset = asset!("/assets/board.css");
const HAND_CSS: Asset = asset!("/assets/hand.css");
const MENU_CSS: Asset = asset!("/assets/menu.css");
const ANIMATIONS_CSS: Asset = asset!("/assets/animations.css");

#[cfg(feature = "server")]
fn main() {
    server::run();
}

#[cfg(not(feature = "server"))]
fn main() {
    dioxus::launch(app);
}

fn app() -> Element {
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
