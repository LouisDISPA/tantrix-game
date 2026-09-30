//! Shown inside the online game route while we've transitioned past the
//! lobby but haven't received the first `GameStarted` snapshot yet.

use dioxus::prelude::*;

use crate::components::menu_card::MenuCard;
use crate::components::menu_screen::MenuScreen;

#[component]
pub fn WaitingForGame() -> Element {
    rsx! {
        MenuScreen {
            MenuCard {
                title: "Waiting for game…",
                subtitle: "The match is about to start",
                p { class: "lobby-waiting", "Waiting for the first game update…" }
            }
        }
    }
}
