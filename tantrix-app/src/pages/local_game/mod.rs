mod screens;

use dioxus::prelude::*;

use crate::state::config::GameConfig;
use crate::state::game_state::{GameContext, GameState};
use crate::state::ui_state::UiState;

pub use screens::game_play::GamePlay;
pub use screens::player_setup::PlayerSetup;

#[derive(Clone, Copy, PartialEq)]
pub enum Screen {
    Setup,
    Playing,
}

/// Context shared across all screens within the local game page.
/// Owns the game state stores so they survive screen transitions.
#[derive(Clone, Copy)]
pub struct LocalGameContext {
    pub screen: Signal<Screen>,
    pub game_state: Store<GameState>,
    pub ui_state: Store<UiState>,
}

#[component]
pub fn LocalGame() -> Element {
    let screen = use_signal(|| Screen::Setup);
    let game_state = use_store(|| GameState::new(&GameConfig::new()));
    let ui_state = use_store(UiState::default);

    use_context_provider(|| LocalGameContext {
        screen,
        game_state,
        ui_state,
    });

    // Also provide GameContext so reusable components (board, hands, etc.)
    // can access game_state/ui_state without knowing about LocalGameContext.
    use_context_provider(|| GameContext {
        game_state,
        ui_state,
    });

    let current_screen = *screen.read();
    match current_screen {
        Screen::Setup => rsx! {
            PlayerSetup {}
        },
        Screen::Playing => rsx! {
            GamePlay {}
        },
    }
}
