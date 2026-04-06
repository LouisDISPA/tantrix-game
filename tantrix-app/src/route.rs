use dioxus::prelude::*;

use crate::components::{game_over::GameOver, game_screen::GameScreen, main_menu::MainMenu, player_setup::PlayerSetup};

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    #[layout(AppLayout)]
    #[route("/")]
    MainMenu {},
    #[route("/setup")]
    PlayerSetup {},
    #[route("/game")]
    GameScreen {},
    #[route("/gameover")]
    GameOver {},
}

#[component]
fn AppLayout() -> Element {
    rsx! {
        Outlet::<Route> {}
    }
}
