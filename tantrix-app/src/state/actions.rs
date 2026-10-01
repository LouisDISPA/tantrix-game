//! Turn actions triggered by UI events.
//!
//! Functions here take a `GameCtx` (one `Copy` struct) and drive the state
//! machine: select a tile, click a position, pick a rotation, play a move,
//! then advance the turn. Playing a move and ending the game go through the
//! context's [`GameDriver`]: a local game changes `game_state` here, an
//! online room is sent the intent and its next snapshot drives the next
//! render.
//!
//! Turn-advance does *not* spawn the AI loop itself. Spawning from a leaf
//! component's click handler ties the task to whichever component was
//! firing (e.g. a `HandTile` that may unmount when the hand shrinks),
//! which would cancel the AI task mid-move. Instead, `advance_turn` just
//! flips the phase to `WaitingForAi`; the game page owns a `use_effect`
//! (installed by `hooks::use_game_ctx`) that reacts to that phase and spawns
//! the AI loop in its own stable scope.

use dioxus::prelude::*;
use tantrix_game::{Color, Placement, Player, Position, Rotation};

use crate::state::config::GameConfig;
use crate::state::game_driver::GameDriver;
use crate::state::game_state::{GameCtx, GameState};
use crate::state::ui_state::{
    set_if_changed, TurnPhase, UiState, UiStateStoreExt, UiStateStoreImplExt,
};

/// The status message of a turn that must fill a forced space.
const FORCED_MESSAGE: &str = "Forced move! You must fill a forced space.";

/// Deal a new local game from `config`, with a fresh UI state, and hand the
/// first turn on: to the AI loop when an AI seat starts. Called from the
/// setup screen's Start Game, so no component writes state while rendering.
pub fn start_game(ctx: GameCtx, config: &GameConfig) {
    let mut game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;
    game_state.set(GameState::new(config));
    ui_state.set(UiState::default());
    advance_turn(ctx);
}

/// Play the current player's tile at `tile_idx` at `pos` in `rotation`.
/// No-op if the tile index is out of range.
pub fn execute_move(ctx: GameCtx, tile_idx: usize, pos: Position, rotation: Rotation) {
    let (player_color, tile) = {
        let gs = ctx.game_state.read();
        let color = gs.current_player();
        let Some(&tile) = gs.hand(color).get(tile_idx) else {
            return;
        };
        (color, tile)
    };
    let placement = Placement {
        tile,
        position: pos,
        rotation,
    };

    match ctx.driver {
        GameDriver::Local => play_here(ctx, player_color, placement),
        // The room plays it and its next snapshot drives the next render; the
        // local `game_state` stays untouched until then.
        GameDriver::Online(room) => {
            room.play(player_color, placement);
            let mut ui_state = ctx.ui_state;
            ui_state.reset_selection();
        }
    }
}

/// Play `placement` for `player` on this device's game, then hand off to
/// [`advance_turn`].
fn play_here(ctx: GameCtx, player: Color, placement: Placement) {
    let mut game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;

    let result = game_state.write().play_move(placement);
    let played = match result {
        Ok(played) => played,
        Err(e) => {
            ui_state.message().set(format!("Error: {}", e));
            return;
        }
    };

    ui_state.after_move(player, played.hand_index, placement.position);
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
            .hand_info()
            .get(tile_idx)
            .and_then(|info| info.positions.get(&pos))
            .cloned();
        (rots, gs.board().is_empty())
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

/// End the game early: on this device for a local game, or for everyone in
/// an online room (the room's `GameEnded` then shows the results).
pub fn end_game(ctx: GameCtx) {
    match ctx.driver {
        GameDriver::Local => {
            let ui_state = ctx.ui_state;
            ui_state.phase().set(TurnPhase::GameOver);
            ui_state.message().set("Game ended early.".to_string());
        }
        GameDriver::Online(room) => room.end_game(),
    }
}

/// Pass the turn on after a move or an AI turn: skip the players who cannot
/// move, then hand the turn to the AI loop if an AI seat has a move to play,
/// or else to a human (or end the game). Online rooms are
/// server-authoritative: the room does all of this, and nothing happens here.
pub fn advance_turn(ctx: GameCtx) {
    let ui_state = ctx.ui_state;
    if !ctx.driver.runs_turns() || *ui_state.phase().read() == TurnPhase::GameOver {
        return;
    }
    let mut game_state = ctx.game_state;

    // Only a stuck player changes the game: otherwise leave the signal
    // (and everything that reads it) alone.
    let stuck = game_state.read().is_stuck();
    if stuck {
        game_state.write().skip_stuck_players();
    }

    let ai_next = ai_to_play(&game_state.read());
    if ai_next {
        // Signal the AI's turn. The game page's `use_effect` owns the spawn
        // so the task lives in the page scope, not this onclick's leaf.
        ui_state.phase().set(TurnPhase::WaitingForAi);
    } else {
        settle_turn(ctx);
    }
}

/// Whether the current player is an AI with a move to play.
fn ai_to_play(gs: &GameState) -> bool {
    matches!(gs.player(gs.current_player()), Player::Ai(_)) && !gs.legal_moves().is_empty()
}

/// Give the turn to a human, or end the game when nobody can move: set the
/// phase and message, and clear stale selections.
fn settle_turn(ctx: GameCtx) {
    let mut ui_state = ctx.ui_state;

    let (is_over, has_forced, has_moves) = {
        let gs = ctx.game_state.read();
        (
            gs.is_game_over(),
            gs.hand_info().iter().any(|info| info.has_forced),
            gs.hand_info().iter().any(|info| info.has_moves()),
        )
    };

    if is_over || !has_moves {
        set_if_changed(ui_state.phase(), TurnPhase::GameOver);
        set_if_changed(ui_state.message(), "Game Over!".to_string());
        set_if_changed(ui_state.new_tile(), None);
    } else if has_forced {
        ui_state.reset_selection_with_message(FORCED_MESSAGE);
    } else {
        ui_state.reset_selection();
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;
    use tantrix_game::Color;

    use super::advance_turn;
    use crate::hooks::use_game_ctx;
    use crate::state::config::{GameConfig, PlayerKind};
    use crate::state::game_driver::GameDriver;
    use crate::state::game_state::{GameCtx, GameState};
    use crate::state::ui_state::{TurnPhase, UiStateStoreExt};

    /// A seeded local game, played with the first legal move each turn, to
    /// the point where Red, to play, cannot move (the bag is empty and Red has
    /// no tile that fits, typically none left) while Blue, seated as `blue`,
    /// still can.
    fn red_stuck_before(blue: PlayerKind) -> GameState {
        (0..100)
            .find_map(|seed| {
                let mut config = GameConfig::new(Some(seed));
                if blue == PlayerKind::Ai {
                    // Human → AI, as the setup screen's toggle does.
                    config.cycle_player(Color::Blue);
                }
                let mut game = GameState::new(&config);
                while !game.is_stuck() {
                    let m = *game.legal_moves().first()?;
                    game.play_move(m.placement()).ok()?;
                }
                let blue_can_move = !game.clone().skip_stuck_players();
                (game.current_player() == Color::Red && blue_can_move).then_some(game)
            })
            .expect("a seed leaves Red stuck while Blue can move")
    }

    /// Advance the turn on a local game page showing `game`; returns the
    /// phase and the player to move afterwards.
    fn advance_on_local_page(game: GameState) -> (TurnPhase, Color) {
        let mut dom = VirtualDom::new_with_props(
            |game: GameState| {
                use_game_ctx(GameDriver::Local, move || game);
                rsx! {}
            },
            game,
        );
        dom.rebuild_in_place();
        dom.in_scope(ScopeId::APP, || {
            let ctx = consume_context::<GameCtx>();
            advance_turn(ctx);
            let phase = *ctx.ui_state.phase().peek();
            let current = ctx.game_state.peek().current_player();
            (phase, current)
        })
    }

    #[test]
    fn a_skipped_human_hands_the_turn_to_the_ai() {
        let (phase, current) = advance_on_local_page(red_stuck_before(PlayerKind::Ai));
        // The AI loop takes Blue's turn, rather than leaving its tiles for
        // the human to click.
        assert_eq!(current, Color::Blue);
        assert_eq!(phase, TurnPhase::WaitingForAi);
    }

    #[test]
    fn a_skipped_human_hands_the_turn_to_the_next_human() {
        let (phase, current) = advance_on_local_page(red_stuck_before(PlayerKind::Human));
        assert_eq!(current, Color::Blue);
        assert_eq!(phase, TurnPhase::SelectTile);
    }
}
