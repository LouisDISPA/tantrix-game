mod screens;

use dioxus::prelude::*;

use crate::state::config::GameConfig;
use crate::state::game_state::{GameContext, GameState};
use crate::state::ui_state::{TurnPhase, UiState, UiStateStoreExt};

pub use screens::game_over::GameOver;
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

    // When playing, split routing between live play and game-over so each
    // screen owns its own layout and navigation.
    let current = *screen.read();
    let phase = ui_state.phase()();
    match (current, phase) {
        (Screen::Setup, _) => rsx! { PlayerSetup {} },
        (Screen::Playing, TurnPhase::GameOver) => rsx! { GameOver {} },
        (Screen::Playing, _) => rsx! { GamePlay {} },
    }
}
