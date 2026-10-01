use std::time::Duration;

use dioxus::prelude::*;
use tantrix_game::Player;

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
            let is_ai =
                !gs.is_game_over() && matches!(gs.player(gs.current_player()), Player::Ai(_));
            (is_ai, !gs.legal_moves().is_empty())
        };
        if !is_ai {
            break;
        }
        if !has_moves {
            // Pass over everyone who cannot move. Once a whole round has no
            // move the game has stalled, and `settle_turn` ends it.
            if game_state.write().skip_stuck_players() {
                break;
            }
            continue;
        }

        let (chosen, ai_color, tile_idx) = {
            let gs = game_state.read();
            let ai_color = gs.current_player();
            let chosen = gs.ai_choose();
            // Where the chosen tile is in the hand, to show it picked.
            let tile_idx = chosen.and_then(|m| gs.hand(ai_color).iter().position(|&t| t == m.tile));
            (chosen, ai_color, tile_idx)
        };
        let Some(chosen) = chosen else { break };

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

        let result = game_state.write().play_move(chosen.placement());
        let played = match result {
            Ok(played) => played,
            Err(err) => {
                ui_state.message().set(format!("AI move failed: {err}"));
                break;
            }
        };
        {
            let mut ui = ui_state.write();
            ui.new_tile = Some((ai_color, played.hand_index));
            ui.last_placed_position = Some(chosen.position);
            ui.selected_tile = None;
            ui.selected_position = None;
            ui.pan_x = 0.0;
            ui.pan_y = 0.0;
        }

        sleep(Duration::from_millis(300)).await;
        if !ai_can_continue(ctx, game_state.read().current_player()) {
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
        && gs.current_player() == expected_color
        && matches!(gs.player(expected_color), Player::Ai(_))
}
