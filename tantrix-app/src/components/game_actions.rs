use std::time::Duration;

use dioxus::prelude::*;
use tantrix_game::{Move, Position, Rotation};

use crate::state::game_state::{GameState, GameStateStoreExt};
use crate::state::ui_state::{TurnPhase, UiState, UiStateStoreExt};

/// Execute a move: build it, play it, then advance the turn.
/// All signal reads happen before any writes to avoid borrow conflicts.
pub fn execute_move(
    tile_idx: usize,
    pos: Position,
    rotation: Rotation,
    mut game_state: Store<GameState>,
    mut ui_state: Store<UiState>,
) {
    // --- Phase 1: build the Move from field-level reads ---
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
    let m = Move {
        tile,
        position: pos,
        rotation,
        forced,
    };

    // --- Phase 2: play the move (single write, then release) ---
    let result = {
        let mut gs = game_state.write();
        gs.play_move(m)
    };
    if let Err(e) = result {
        ui_state.message().set(format!("Error: {}", e));
        return;
    }

    // --- Phase 3: smooth recenter + mark new tile for animation ---
    ui_state.write().after_move(player_color, tile_idx, m.position);

    // --- Phase 4: advance the turn with clean read→write cycles ---
    advance_turn(game_state, ui_state);
}

pub fn handle_position_click(
    pos: Position,
    game_state: Store<GameState>,
    ui_state: Store<UiState>,
) {
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
    let tile_idx = match tile_idx {
        Some(idx) => idx,
        None => return,
    };
    let rotations = match rotations {
        Some(rots) => rots,
        None => return,
    };

    let board_empty = game_state.board().read().is_empty();

    if rotations.len() == 1 || board_empty {
        execute_move(tile_idx, pos, rotations[0], game_state, ui_state);
    } else {
        ui_state.selected_position().set(Some(pos));
        ui_state.phase().set(TurnPhase::SelectRotation);
    }
}

pub fn handle_rotation_select(
    rotation: Rotation,
    game_state: Store<GameState>,
    ui_state: Store<UiState>,
) {
    let (tile_idx, pos) = (
        *ui_state.selected_tile().read(),
        *ui_state.selected_position().read(),
    );
    let tile_idx = match tile_idx {
        Some(idx) => idx,
        None => return,
    };
    let pos = match pos {
        Some(p) => p,
        None => return,
    };

    execute_move(tile_idx, pos, rotation, game_state, ui_state);
}

/// After a successful move, check if AI should play next.
pub fn advance_turn(game_state: Store<GameState>, ui_state: Store<UiState>) {
    let needs_ai = {
        let gs = game_state.read();
        !gs.is_game_over()
            && matches!(
                gs.players[gs.turn_tracker.current()],
                tantrix_game::Player::Ai(_)
            )
    };

    if needs_ai {
        ui_state.phase().set(TurnPhase::WaitingForAi);
        spawn(async move {
            ai_step_loop(game_state, ui_state).await;
            settle_turn(game_state, ui_state);
        });
    } else {
        settle_turn(game_state, ui_state);
    }
}

async fn ai_step_loop(mut game_state: Store<GameState>, mut ui_state: Store<UiState>) {
    use crate::util::sleep;

    loop {
        let (is_ai, has_moves) = {
            let gs = game_state.read();
            let is_ai = !gs.is_game_over()
                && matches!(
                    gs.players[gs.turn_tracker.current()],
                    tantrix_game::Player::Ai(_)
                );
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
            let color = gs.turn_tracker.current();
            (gs.ai_choose(), color)
        };
        let Some(chosen) = chosen else { break };

        let tile_idx = {
            let hands = game_state.hands();
            let h = hands.read();
            let current = game_state.turn_tracker().read().current();
            h[current].position(&chosen.tile)
        };
        if let Some(idx) = tile_idx {
            ui_state.selected_tile().set(Some(idx));
            ui_state.phase().set(TurnPhase::WaitingForAi);
        }

        sleep(Duration::from_millis(500)).await;

        ui_state.selected_position().set(Some(chosen.position));
        sleep(Duration::from_millis(400)).await;

        {
            let mut gs = game_state.write();
            let _ = gs.play_move(chosen);
        }
        {
            let mut ui = ui_state.write();
            ui.new_tile = tile_idx.map(|idx| (ai_color, idx));
            ui.last_placed_position = Some(chosen.position);
            ui.selected_tile = None;
            ui.selected_position = None;
            ui.animating = true;
            ui.pan_x = 0.0;
            ui.pan_y = 0.0;
        }

        sleep(Duration::from_millis(300)).await;
    }
}

fn settle_turn(mut game_state: Store<GameState>, mut ui_state: Store<UiState>) {
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
