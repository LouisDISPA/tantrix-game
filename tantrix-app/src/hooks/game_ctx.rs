//! The one place a game page builds its `GameCtx`.

use dioxus::prelude::*;

use crate::state::actions::advance_turn;
use crate::state::ai_service::run_ai_loop;
use crate::state::game_driver::GameDriver;
use crate::state::game_state::{GameCtx, GameState};
use crate::state::ui_state::{set_if_changed, TurnPhase, UiState, UiStateStoreExt};

/// Create a game page's state (`game`, a fresh `UiState`, played by
/// `driver`) and provide it as the `GameCtx` of every component below.
/// `LocalGame` and `OnlineGameView` each call it once.
///
/// When the driver runs the turns here (a local game), this also plays the
/// AI seats: `advance_turn` flips the phase to `WaitingForAi` whenever an AI
/// needs to play, and an effect reacts to that transition by spawning the AI
/// loop in the calling page's scope, where the context's signals are owned.
/// The task is cancelled if the user navigates off the page.
pub fn use_game_ctx(driver: GameDriver, game: impl FnOnce() -> GameState) -> GameCtx {
    let game_state = use_signal(game);
    let ui_state = use_store(UiState::default);
    let ctx = use_context_provider(move || GameCtx {
        game_state,
        ui_state,
        driver,
    });
    use_ai_turns(ctx);
    // A tile gesture never survives a changed hand or seat.
    use_effect(move || {
        let game = ctx.game_state.read();
        let phase = ctx.ui_state.phase()();
        let drag = *ctx.ui_state.tile_drag().peek();
        if let Some(drag) = drag {
            if !drag.is_current(&game)
                || !ctx.driver.controls(drag.color)
                || matches!(phase, TurnPhase::WaitingForAi | TurnPhase::GameOver)
            {
                set_if_changed(ctx.ui_state.tile_drag(), None);
                set_if_changed(ctx.ui_state.suppress_tile_click(), true);
            }
        }
    });
    ctx
}

/// Spawn the AI loop whenever the phase turns to `WaitingForAi`.
/// `ai_in_flight` guards against spurious double-spawns if the effect re-runs
/// while the AI is already running.
fn use_ai_turns(ctx: GameCtx) {
    let mut ai_in_flight = use_signal(|| false);
    use_effect(move || {
        if !ctx.driver.runs_turns() {
            return;
        }
        let is_waiting = *ctx.ui_state.phase().read() == TurnPhase::WaitingForAi;
        let busy = *ai_in_flight.read();
        if !is_waiting || busy {
            return;
        }
        ai_in_flight.set(true);
        spawn(async move {
            run_ai_loop(ctx).await;
            // Hand the turn on: to a human, or to the next AI seat, which
            // this effect picks up again once `ai_in_flight` clears.
            advance_turn(ctx);
            ai_in_flight.set(false);
        });
    });
}
