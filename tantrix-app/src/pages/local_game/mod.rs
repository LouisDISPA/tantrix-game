mod screens;

use dioxus::prelude::*;

use crate::state::config::GameConfig;
use crate::state::game_state::{GameCtx, GameState};
use crate::state::ui_state::{Screen, TurnPhase, UiState, UiStateStoreExt};

pub use screens::game_over::GameOver;
pub use screens::game_play::GamePlay;
pub use screens::player_setup::PlayerSetup;

#[component]
pub fn LocalGame() -> Element {
    let screen = use_signal(|| Screen::Setup);
    let game_state = use_signal(|| GameState::new(&GameConfig::new()));
    let ui_state = use_store(UiState::default);
    let online = use_signal(|| None);

    use_context_provider(|| GameCtx {
        screen,
        game_state,
        ui_state,
        online,
    });

    let current = *screen.read();
    let phase = ui_state.phase()();
    match (current, phase) {
        (Screen::Setup, _) => rsx! { PlayerSetup {} },
        (Screen::Playing, TurnPhase::GameOver) => rsx! { GameOver {} },
        (Screen::Playing, _) => rsx! { GamePlay {} },
    }
}
