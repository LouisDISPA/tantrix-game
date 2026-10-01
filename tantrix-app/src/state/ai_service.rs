use std::time::Duration;

use dioxus::prelude::*;
use tantrix_game::Player;

use crate::state::game_state::GameCtx;
use crate::state::ui_state::{set_if_changed, TurnPhase, UiStateStoreExt, UiStateStoreImplExt};
use crate::util::sleep;

/// Run AI players' turns with cinematic pacing: select the tile, sweep to
/// the position, commit the move, pause, then look at the next player. Exits
/// when the current player is a human, an AI with no move, or the game is
/// over; [`advance_turn`](crate::state::actions::advance_turn) then skips
/// whoever cannot move and hands the turn on (back to this loop if an AI
/// seat can move).
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

        // `None` for a human, an AI with no legal move, or a finished game.
        let (chosen, ai_color, tile_idx) = {
            let gs = game_state.read();
            let ai_color = gs.current_player();
            let chosen = gs.ai_choose();
            // Where the chosen tile is in the hand, to show it picked.
            let tile_idx = chosen.and_then(|m| gs.hand(ai_color).iter().position(|&t| t == m.tile));
            (chosen, ai_color, tile_idx)
        };
        let Some(chosen) = chosen else { break };

        // The phase already is `WaitingForAi` (checked above): only the
        // selection changes, so only its readers wake.
        if let Some(idx) = tile_idx {
            ui_state.selected_tile().set(Some(idx));
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
        ui_state.after_move(ai_color, played.hand_index, chosen.position);
        set_if_changed(ui_state.selected_tile(), None);
        set_if_changed(ui_state.selected_position(), None);

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
