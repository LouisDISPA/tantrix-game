mod player_setup;

use dioxus::prelude::*;

use crate::hooks::use_game_ctx;
use crate::pages::game::{GameOver, GamePlay};
use crate::state::config::GameConfig;
use crate::state::game_driver::GameDriver;
use crate::state::game_state::GameState;
use crate::state::ui_state::{Screen, TurnPhase, UiState, UiStateStoreExt};

use player_setup::PlayerSetup;

/// Setup → Playing → GameOver, with every seat played on this device.
/// `seed` (from `/game?seed=N`) deals the same game every time.
#[component]
pub fn LocalGame(seed: Option<u64>) -> Element {
    let mut screen = use_signal(|| Screen::Setup);
    let ctx = use_game_ctx(GameDriver::Local, || GameState::new(&GameConfig::new(seed)));
    let mut game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;

    let current = *screen.read();
    let phase = ui_state.phase()();
    match (current, phase) {
        (Screen::Setup, _) => rsx! {
            PlayerSetup {
                seed,
                on_start: move |config: GameConfig| {
                    game_state.set(GameState::new(&config));
                    ui_state.set(UiState::default());
                    screen.set(Screen::Playing);
                },
            }
        },
        // Play Again resets UI state and returns to the setup screen.
        (Screen::Playing, TurnPhase::GameOver) => rsx! {
            GameOver {
                again_label: "Play Again",
                on_again: move |_| {
                    ui_state.set(UiState::default());
                    screen.set(Screen::Setup);
                },
            }
        },
        (Screen::Playing, _) => rsx! { GamePlay {} },
    }
}
