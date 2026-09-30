mod screens;

use dioxus::prelude::*;

use crate::state::actions::settle_turn;
use crate::state::ai_service::run_ai_loop;
use crate::state::config::GameConfig;
use crate::state::game_state::{GameCtx, GameState};
use crate::state::ui_state::{Screen, TurnPhase, UiState, UiStateStoreExt};

pub use screens::game_over::GameOver;
pub use screens::game_play::GamePlay;
pub use screens::player_setup::PlayerSetup;

/// `seed` (from `/game?seed=N`) deals the same game every time.
#[component]
pub fn LocalGame(seed: Option<u64>) -> Element {
    let screen = use_signal(|| Screen::Setup);
    let game_state = use_signal(|| GameState::new(&GameConfig::new(seed)));
    let ui_state = use_store(UiState::default);
    let online = use_signal(|| None);
    let my_color = use_signal(|| None);
    let player_names = use_signal(tantrix_game::EnumMap::default);

    let ctx = GameCtx {
        screen,
        game_state,
        ui_state,
        online,
        my_color,
        player_names,
    };
    use_context_provider(move || ctx);

    // AI driver: `advance_turn` flips the phase to `WaitingForAi` whenever
    // an AI needs to play. This effect reacts to that transition and
    // spawns the AI loop in `LocalGame`'s scope, where `GameCtx`'s signals
    // are owned. The task is automatically cancelled if the user
    // navigates off the page. `ai_in_flight` guards against spurious
    // double-spawns if the effect re-runs while the AI is already running.
    let mut ai_in_flight = use_signal(|| false);
    use_effect(move || {
        let is_waiting = *ui_state.phase().read() == TurnPhase::WaitingForAi;
        let is_online = online.read().is_some();
        let busy = *ai_in_flight.read();
        if !is_waiting || is_online || busy {
            return;
        }
        ai_in_flight.set(true);
        spawn(async move {
            run_ai_loop(ctx).await;
            settle_turn(ctx);
            ai_in_flight.set(false);
        });
    });

    let current = *screen.read();
    let phase = ui_state.phase()();
    match (current, phase) {
        (Screen::Setup, _) => rsx! { PlayerSetup { seed } },
        (Screen::Playing, TurnPhase::GameOver) => rsx! { GameOver {} },
        (Screen::Playing, _) => rsx! { GamePlay {} },
    }
}
