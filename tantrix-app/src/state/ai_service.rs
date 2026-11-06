use std::time::Duration;

use dioxus::prelude::*;
use tantrix_game::{Color, Move, MoveError, Player, Position};

use crate::state::actions::frame_board;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{set_if_changed, TurnPhase, UiStateStoreExt, UiStateStoreImplExt};
use crate::util::sleep;

/// Run AI players' turns with cinematic pacing: pause before thinking,
/// select the tile, sweep to the position, commit the move, pause, then
/// look at the next player. Exits when the current player is a human, an AI
/// with no move, or the game is over;
/// [`advance_turn`](crate::state::actions::advance_turn) then skips
/// whoever cannot move and hands the turn on (back to this loop if an AI
/// seat can move).
///
/// Designed to be `spawn`ed — scalar signal reads happen before any `.await`
/// to avoid holding guards across suspend points.
pub async fn run_ai_loop(ctx: GameCtx) {
    let game_state = ctx.game_state;
    let ui_state = ctx.ui_state;

    loop {
        if *ui_state.phase().read() != TurnPhase::WaitingForAi {
            break;
        }

        let ai_color = game_state.read().current_player();
        // Let the previous move's animations finish and the thinking hint
        // paint before synchronous search blocks the browser's main thread.
        sleep(Duration::from_millis(500)).await;
        if !ai_can_continue(ctx, ai_color) {
            break;
        }

        // `None` for a human, an AI with no legal move, or a finished game.
        let (chosen, tile_idx) = {
            let gs = game_state.read();
            let chosen = gs.ai_choose();
            // Where the chosen tile is in the hand, to show it picked.
            let tile_idx = chosen.and_then(|m| gs.hand(ai_color).iter().position(|&t| t == m.tile));
            (chosen, tile_idx)
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

        if let Err(err) = play_ai_move(ctx, ai_color, chosen) {
            ui_state.message().set(format!("AI move failed: {err}"));
            break;
        }

        sleep(Duration::from_millis(300)).await;
        if !ai_can_continue(ctx, game_state.read().current_player()) {
            break;
        }
    }
}

/// Play `color`'s `chosen` move on the board, then [`after_ai_move`].
fn play_ai_move(ctx: GameCtx, color: Color, chosen: Move) -> Result<(), MoveError> {
    let mut game_state = ctx.game_state;
    let played = game_state.write().play_move(chosen.placement())?;
    after_ai_move(ctx, color, played.hand_index, chosen.position);
    Ok(())
}

/// An AI's move has landed: mark it, put the board back in the camera as
/// after a move played here, and drop the AI's selection.
fn after_ai_move(ctx: GameCtx, color: Color, hand_index: usize, at: Position) {
    let ui_state = ctx.ui_state;
    let mut ui = ui_state;
    ui.after_move(color, hand_index, at);
    frame_board(ctx);
    set_if_changed(ui_state.selected_tile(), None);
    set_if_changed(ui_state.selected_position(), None);
}

fn ai_can_continue(ctx: GameCtx, expected_color: Color) -> bool {
    let phase_is_ai = *ctx.ui_state.phase().read() == TurnPhase::WaitingForAi;
    if !phase_is_ai {
        return false;
    }
    let gs = ctx.game_state.read();
    !gs.is_game_over()
        && gs.current_player() == expected_color
        && matches!(gs.player(expected_color), Player::Ai(_))
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;

    use tantrix_game::{Color, Position};

    use super::{after_ai_move, play_ai_move};
    use crate::hooks::camera::BoardCamera;
    use crate::state::actions::execute_move;
    use crate::state::config::GameConfig;
    use crate::state::game_state::GameState;
    use crate::state::test_games::seeded;
    use crate::state::ui_state::UiStateStoreExt;
    use crate::svg::geometry::Rect;
    use crate::util::test_page::GamePage;

    #[test]
    fn an_ais_move_puts_the_board_back_in_the_camera() {
        // A long game: its board no longer fits a phone's stage at 100%.
        let mut game = seeded(3);
        for _ in 0..30 {
            game.skip_stuck_players();
            let Some(m) = game.legal_moves().first().copied() else {
                break;
            };
            game.play_move(m.placement()).unwrap();
        }
        let mover = game.current_player();
        let at = game.board().iter().last().unwrap().position;
        let mut page = GamePage::local(game);
        page.act(|ctx| {
            ctx.ui_state
                .stage()
                .set(Some(Rect::new(0.0, 52.0, 390.0, 652.0)));
            ctx.ui_state.selected_tile().set(Some(2));
        });

        page.act(|ctx| after_ai_move(ctx, mover, 2, at));
        let (pose, selected) = page.with(|ctx| {
            (
                BoardCamera::new(ctx.ui_state).pose(),
                *ctx.ui_state.selected_tile().peek(),
            )
        });
        assert!(pose.zoom < 1.0, "{pose:?}");
        assert_eq!(selected, None);
    }

    /// Where the page marks the last move.
    fn marked(page: &GamePage) -> Option<Position> {
        page.with(|ctx| *ctx.ui_state.last_placed_position().peek())
    }

    #[test]
    fn an_ais_move_stays_marked_until_the_next_move() {
        // Red plays here, Blue is an AI; Red has opened, so Blue is to move.
        let mut config = GameConfig::new(Some(0));
        config.cycle_player(Color::Blue);
        let mut game = GameState::new(&config);
        if game.current_player() == Color::Red {
            game.play_move(game.legal_moves()[0].placement()).unwrap();
        }
        assert_eq!(game.current_player(), Color::Blue);
        let mut page = GamePage::local(game);
        assert_eq!(marked(&page), None);

        // The AI loop's commit step, without its pauses.
        let chosen = page.game(|game| game.ai_choose()).unwrap();
        page.act(|ctx| play_ai_move(ctx, Color::Blue, chosen).unwrap());
        assert_eq!(marked(&page), Some(chosen.position));

        // Red answers: the mark moves to Red's tile. (`with`, not `act`: the
        // AI's next turn, and its timers, would start.)
        let (m, idx) = page.game(|game| {
            assert_eq!(game.current_player(), Color::Red);
            let m = game.legal_moves()[0];
            let idx = game.hand(Color::Red).iter().position(|&t| t == m.tile);
            (m, idx.unwrap())
        });
        page.with(|ctx| execute_move(ctx, idx, m.position, m.rotation));
        assert_ne!(m.position, chosen.position);
        assert_eq!(marked(&page), Some(m.position));
    }
}
