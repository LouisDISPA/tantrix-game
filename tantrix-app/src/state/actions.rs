//! Turn actions triggered by UI events.
//!
//! Functions here take a `GameCtx` (one `Copy` struct) and drive the state
//! machine: select a tile, click a position, pick a rotation, play a move,
//! then advance the turn.
//!
//! Turn-advance does *not* spawn the AI loop itself. Spawning from a leaf
//! component's click handler ties the task to whichever component was
//! firing (e.g. a `HandTile` that may unmount when the hand shrinks),
//! which would cancel the AI task mid-move. Instead, `advance_turn` just
//! flips the phase to `WaitingForAi`; the `LocalGame` page owns a
//! `use_effect` that reacts to that phase and spawns the AI loop in its
//! own stable scope.

use dioxus::prelude::*;
use tantrix_game::{Move, Player, Position, Rotation};
use tantrix_net::SessionCommand;

use crate::state::game_state::GameCtx;
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};

/// Build a move from the current selection, play it, and hand off to
/// [`advance_turn`]. No-op if the tile index is out of range.
pub fn execute_move(ctx: GameCtx, tile_idx: usize, pos: Position, rotation: Rotation) {
    let mut game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;

    let (player_color, tile, forced) = {
        let gs = game_state.read();
        let color = gs.turn_tracker.current();
        let Some(&tile) = gs.hands[color].get(tile_idx) else {
            return;
        };
        let forced = gs
            .hand_info
            .get(tile_idx)
            .map(|info| info.has_forced)
            .unwrap_or(false);
        (color, tile, forced)
    };
    let m = Move {
        tile,
        position: pos,
        rotation,
        forced,
    };

    // Online play: forward the intent and let the host's StateUpdate drive
    // the next render. The local GameState stays untouched until then.
    if let Some(sender) = ctx.online.read().as_ref().cloned() {
        // Only the peer who owns the current seat may dispatch a move.
        // Spectators (and non-current seats) can hover to preview but
        // their click handlers are gated upstream — this is the
        // belt-and-suspenders check.
        if *ctx.my_color.read() != Some(player_color) {
            ui_state.write().reset_selection();
            return;
        }
        sender.send(SessionCommand::PlayMove(m));
        ui_state.write().reset_selection();
        return;
    }

    let result = game_state.write().play_move(m);
    if let Err(e) = result {
        ui_state.message().set(format!("Error: {}", e));
        return;
    }

    ui_state
        .write()
        .after_move(player_color, tile_idx, m.position);
    advance_turn(ctx);
}

/// React to a click on an empty board position. If the tile only fits in one
/// rotation at this position, commit immediately; otherwise advance to
/// `SelectRotation` phase and let the wheel pick.
pub fn click_position(ctx: GameCtx, pos: Position) {
    let ui_state = ctx.ui_state;
    let game_state = ctx.game_state;

    let phase = *ui_state.phase().read();
    if phase != TurnPhase::SelectPosition {
        return;
    }
    let Some(tile_idx) = *ui_state.selected_tile().read() else {
        return;
    };

    let (rotations, board_empty) = {
        let gs = game_state.read();
        let rots = gs
            .hand_info
            .get(tile_idx)
            .and_then(|info| info.positions.get(&pos))
            .cloned();
        (rots, gs.board.is_empty())
    };
    let Some(rotations) = rotations else { return };

    if rotations.len() == 1 || board_empty {
        execute_move(ctx, tile_idx, pos, rotations[0]);
    } else {
        ui_state.selected_position().set(Some(pos));
        ui_state.phase().set(TurnPhase::SelectRotation);
    }
}

/// Commit the move after the user picks a rotation from the wheel.
pub fn pick_rotation(ctx: GameCtx, rotation: Rotation) {
    let ui_state = ctx.ui_state;
    let tile_idx = *ui_state.selected_tile().read();
    let pos = *ui_state.selected_position().read();
    let Some(tile_idx) = tile_idx else { return };
    let Some(pos) = pos else { return };
    execute_move(ctx, tile_idx, pos, rotation);
}

/// Run any end-of-turn bookkeeping. If the next active player is an AI, kick
/// off the async AI loop; otherwise settle directly.
pub fn advance_turn(ctx: GameCtx) {
    let ui_state = ctx.ui_state;

    // Online rooms are host-authoritative: we never schedule AI turns or
    // mutate local state here.
    if ctx.online.read().is_some() {
        return;
    }

    let needs_ai = {
        let gs = ctx.game_state.read();
        !gs.is_game_over() && matches!(gs.players[gs.turn_tracker.current()], Player::Ai(_))
    };

    if needs_ai {
        // Signal the AI's turn. `LocalGame`'s `use_effect` owns the spawn
        // so the task lives in the page scope, not this onclick's leaf.
        ui_state.phase().set(TurnPhase::WaitingForAi);
    } else {
        settle_turn(ctx);
    }
}

/// After control returns to a human (or the game ends), skip past any stuck
/// players, adjust the phase + message, and clear stale selections.
pub fn settle_turn(ctx: GameCtx) {
    let mut game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;

    game_state.write().skip_stuck_players();

    let (is_over, has_forced, has_moves) = {
        let gs = game_state.read();
        (
            gs.is_game_over(),
            gs.hand_info.iter().any(|info| info.has_forced),
            gs.hand_info.iter().any(|info| info.has_moves()),
        )
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
