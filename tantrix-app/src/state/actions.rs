//! Turn actions triggered by UI events.
//!
//! Functions here take a `GameContext` (two `Store`s bundled) and drive the
//! state machine: select a tile, click a position, pick a rotation, play a
//! move, then advance the turn. If the next player is an AI, `advance_turn`
//! spawns [`run_ai_loop`] to take care of their move.
//!
//! All signal reads happen up-front inside each function and borrows are
//! released before any writes, so state mutations don't overlap and we don't
//! hold guards across `.await` in the AI loop.

use dioxus::prelude::*;
use tantrix_game::{Move, Player, Position, Rotation};

use crate::state::ai_service::run_ai_loop;
use crate::state::game_state::{GameContext, GameStateStoreExt};
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};

/// Build a move from the current selection, play it, and hand off to
/// [`advance_turn`]. No-op if the tile index is out of range.
pub fn execute_move(ctx: GameContext, tile_idx: usize, pos: Position, rotation: Rotation) {
    let mut game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;

    let player_color = game_state.turn_tracker().read().current();
    let tile = {
        let hands = game_state.hands();
        let h = hands.read();
        match h[player_color].get(tile_idx) {
            Some(t) => *t,
            None => return,
        }
    };
    let forced = game_state
        .hand_info()
        .read()
        .get(tile_idx)
        .map(|info| info.has_forced)
        .unwrap_or(false);
    let m = Move { tile, position: pos, rotation, forced };

    let result = {
        let mut gs = game_state.write();
        gs.play_move(m)
    };
    if let Err(e) = result {
        ui_state.message().set(format!("Error: {}", e));
        return;
    }

    ui_state.write().after_move(player_color, tile_idx, m.position);
    advance_turn(ctx);
}

/// React to a click on an empty board position. If the tile only fits in one
/// rotation at this position, commit immediately; otherwise advance to
/// `SelectRotation` phase and let the wheel pick.
pub fn click_position(ctx: GameContext, pos: Position) {
    let ui_state = ctx.ui_state;
    let game_state = ctx.game_state;

    let (phase, tile_idx, rotations) = {
        let phase = *ui_state.phase().read();
        let tile_idx = *ui_state.selected_tile().read();
        let hi_store = game_state.hand_info();
        let hi = hi_store.read();
        let rots = tile_idx
            .and_then(|idx| hi.get(idx))
            .and_then(|info| info.positions.get(&pos))
            .cloned();
        (phase, tile_idx, rots)
    };

    if phase != TurnPhase::SelectPosition {
        return;
    }
    let Some(tile_idx) = tile_idx else { return };
    let Some(rotations) = rotations else { return };

    let board_empty = game_state.board()().is_empty();
    if rotations.len() == 1 || board_empty {
        execute_move(ctx, tile_idx, pos, rotations[0]);
    } else {
        ui_state.selected_position().set(Some(pos));
        ui_state.phase().set(TurnPhase::SelectRotation);
    }
}

/// Commit the move after the user picks a rotation from the wheel.
pub fn pick_rotation(ctx: GameContext, rotation: Rotation) {
    let ui_state = ctx.ui_state;
    let tile_idx = *ui_state.selected_tile().read();
    let pos = *ui_state.selected_position().read();
    let Some(tile_idx) = tile_idx else { return };
    let Some(pos) = pos else { return };
    execute_move(ctx, tile_idx, pos, rotation);
}

/// Run any end-of-turn bookkeeping. If the next active player is an AI, kick
/// off the async AI loop; otherwise settle directly.
pub fn advance_turn(ctx: GameContext) {
    let game_state = ctx.game_state;
    let ui_state = ctx.ui_state;

    let needs_ai = {
        let gs = game_state.read();
        !gs.is_game_over()
            && matches!(gs.players[gs.turn_tracker.current()], Player::Ai(_))
    };

    if needs_ai {
        ui_state.phase().set(TurnPhase::WaitingForAi);
        spawn(async move {
            run_ai_loop(ctx).await;
            settle_turn(ctx);
        });
    } else {
        settle_turn(ctx);
    }
}

/// After control returns to a human (or the game ends), skip past any stuck
/// players, adjust the phase + message, and clear stale selections.
fn settle_turn(ctx: GameContext) {
    let mut game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;

    game_state.write().skip_stuck_players();

    let (is_over, has_forced, has_moves) = {
        let gs = game_state.read();
        let is_over = gs.is_game_over();
        let has_forced = gs.hand_info.iter().any(|info| info.has_forced);
        let has_moves = gs.hand_info.iter().any(|info| info.has_moves());
        (is_over, has_forced, has_moves)
    };

    let mut ui = ui_state.write();
    if is_over || !has_moves {
        ui.phase = TurnPhase::GameOver;
        ui.message = "Game Over!".to_string();
        ui.new_tile = None;
    } else if has_forced {
        ui.reset_selection();
        ui.message = "Forced move! You must fill a forced space.".to_string();
    } else {
        ui.reset_selection();
    }
}
