use std::time::Duration;

use dioxus::prelude::*;
use tantrix_game::{Move, Player};

use crate::state::game_state::GameCtx;
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};
use crate::util::sleep;

/// Run an AI player's turn with cinematic pacing: select the tile, sweep to
/// the position, commit the move, pause, then look at the next player. Exits
/// when the current player becomes a human or the game is over.
///
/// Designed to be `spawn`ed — scalar signal reads happen before any `.await`
/// to avoid holding guards across suspend points.
pub async fn run_ai_loop(ctx: GameCtx) {
    let mut game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;

    loop {
        if *ui_state.phase().read() != TurnPhase::WaitingForAi {
            break;
        }
        let (is_ai, has_moves) = {
            let gs = game_state.read();
            let is_ai = !gs.is_game_over()
                && matches!(gs.players[gs.turn_tracker.current()], Player::Ai(_));
            let has_moves = gs.hand_info.iter().any(|info| info.has_moves());
            (is_ai, has_moves)
        };
        if !is_ai {
            break;
        }
        if !has_moves {
            game_state.write().skip_turn();
            continue;
        }

        let (chosen, ai_color) = {
            let gs = game_state.read();
            (gs.ai_choose(), gs.turn_tracker.current())
        };
        let Some(chosen) = chosen else { break };

        let tile_idx = find_tile_index(&ctx, chosen);
        if let Some(idx) = tile_idx {
            ui_state.selected_tile().set(Some(idx));
            ui_state.phase().set(TurnPhase::WaitingForAi);
        }

        sleep(Duration::from_millis(500)).await;
        if !ai_can_continue(ctx, ai_color) {
            break;
        }

        ui_state.selected_position().set(Some(chosen.position));
        sleep(Duration::from_millis(400)).await;
        if !ai_can_continue(ctx, ai_color) {
            break;
        }

        if let Err(err) = game_state.write().play_move(chosen) {
            ui_state.message().set(format!("AI move failed: {err}"));
            break;
        }
        {
            let mut ui = ui_state.write();
            ui.new_tile = tile_idx.map(|idx| (ai_color, idx));
            ui.last_placed_position = Some(chosen.position);
            ui.selected_tile = None;
            ui.selected_position = None;
            ui.pan_x = 0.0;
            ui.pan_y = 0.0;
        }

        sleep(Duration::from_millis(300)).await;
        if !ai_can_continue(ctx, game_state.read().turn_tracker.current()) {
            break;
        }
    }
}

fn ai_can_continue(ctx: GameCtx, expected_color: tantrix_game::Color) -> bool {
    let phase_is_ai = *ctx.ui_state.phase().read() == TurnPhase::WaitingForAi;
    if !phase_is_ai {
        return false;
    }
    let gs = ctx.game_state.read();
    !gs.is_game_over()
        && gs.turn_tracker.current() == expected_color
        && matches!(gs.players[expected_color], Player::Ai(_))
}

fn find_tile_index(ctx: &GameCtx, chosen: Move) -> Option<usize> {
    let gs = ctx.game_state.read();
    let current = gs.turn_tracker.current();
    gs.hands[current].iter().position(|t| t == &chosen.tile)
}
