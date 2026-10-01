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
use tantrix_game::{Color, Placement, Player, Position, Rotation, TileMoveInfo};

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

/// The rotation a tile is played in at a position without the rotation
/// wheel: the only one that fits there, or — on the empty board, where the
/// opening tile fits every way round — the first. `None` when the player
/// picks one.
pub fn auto_rotation(rotations: &[Rotation], board_empty: bool) -> Option<Rotation> {
    match rotations {
        [only] => Some(*only),
        [first, ..] if board_empty => Some(*first),
        _ => None,
    }
}

/// The move a hand tile is played in without the player choosing anything:
/// the tile fits at a single position, in its [`auto_rotation`] there.
/// `None` when the tile has no move, or the player has a choice to make.
pub fn auto_move(info: &TileMoveInfo, board_empty: bool) -> Option<(Position, Rotation)> {
    let mut positions = info.positions.iter();
    let (&position, rotations) = positions.next()?;
    if positions.next().is_some() {
        return None;
    }
    Some((position, auto_rotation(rotations, board_empty)?))
}

/// React to a click on `color`'s hand tile at `idx`. Only the player to move
/// plays their tiles, and only on a device that plays their seat (any seat
/// locally, its own online): `HandTile` offers the click on such a seat
/// only, and this checks again so no caller can play another seat. Clicking
/// the selected tile again puts it back; a tile with a single legal move
/// ([`auto_move`]) is played at once; any other tile is selected, for the
/// player to choose a position. Outside the human turn phases it does
/// nothing.
pub fn click_tile(ctx: GameCtx, color: Color, idx: usize) {
    let mut ui_state = ctx.ui_state;

    let on_turn = ctx.game_state.read().current_player() == color;
    if !on_turn || !ctx.driver.controls(color) {
        return;
    }

    let phase = *ui_state.phase().read();
    match phase {
        TurnPhase::SelectPosition | TurnPhase::SelectRotation => {
            if *ui_state.selected_tile().read() == Some(idx) {
                ui_state.reset_selection();
                return;
            }
        }
        TurnPhase::SelectTile => {}
        TurnPhase::WaitingForAi | TurnPhase::GameOver => return,
    }

    let auto = {
        let gs = ctx.game_state.read();
        let board_empty = gs.board().is_empty();
        gs.hand_info()
            .get(idx)
            .and_then(|info| auto_move(info, board_empty))
    };
    match auto {
        Some((pos, rotation)) => execute_move(ctx, idx, pos, rotation),
        None => ui_state.select_tile(idx),
    }
}

/// React to a click on an empty board position. If the tile only fits in one
/// rotation at this position ([`auto_rotation`]), commit immediately;
/// otherwise advance to `SelectRotation` phase and let the wheel pick.
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

    match auto_rotation(&rotations, board_empty) {
        Some(rotation) => execute_move(ctx, tile_idx, pos, rotation),
        None => {
            ui_state.selected_position().set(Some(pos));
            ui_state.phase().set(TurnPhase::SelectRotation);
        }
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
    use std::collections::BTreeMap;

    use dioxus::prelude::*;
    use futures_channel::mpsc::UnboundedReceiver;
    use tantrix_game::{Color, Direction, EnumMap, Position, Rotation, Tile, TileMoveInfo};
    use tantrix_net::SessionCommand;

    use super::{
        advance_turn, auto_move, auto_rotation, click_position, click_tile, end_game, execute_move,
        pick_rotation,
    };
    use crate::state::config::{GameConfig, PlayerKind};
    use crate::state::game_state::GameState;
    use crate::state::test_games::{find_game, seeded, tile_with_one_move, tile_with_positions};
    use crate::state::ui_state::{TurnPhase, UiStateStoreExt, UiStateStoreImplExt};
    use crate::util::test_page::GamePage;

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
    /// phase and the player to move afterwards. (The page is not let to
    /// catch up: it would start the AI loop's timers.)
    fn advance_on_local_page(game: GameState) -> (TurnPhase, Color) {
        GamePage::local(game).with(|ctx| {
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

    /// What the turn flow shows: the phase, the selected tile and position.
    fn selection(page: &GamePage) -> (TurnPhase, Option<usize>, Option<Position>) {
        page.with(|ctx| {
            (
                *ctx.ui_state.phase().peek(),
                *ctx.ui_state.selected_tile().peek(),
                *ctx.ui_state.selected_position().peek(),
            )
        })
    }

    /// The tile placed at `at`, and how it is turned.
    fn placed_at(page: &GamePage, at: Position) -> Option<(Tile, Rotation)> {
        page.game(|game| {
            let m = game.board().iter().find(|m| m.position == at)?;
            Some((m.tile, m.rotation))
        })
    }

    fn several_positions() -> (GameState, usize, Position) {
        tile_with_positions(|_| true)
    }

    /// The commands an online page has sent the room so far.
    fn sent(commands: &mut UnboundedReceiver<SessionCommand>) -> Vec<SessionCommand> {
        std::iter::from_fn(|| commands.try_recv().ok()).collect()
    }

    fn info(positions: &[(Position, &[Rotation])]) -> TileMoveInfo {
        TileMoveInfo {
            has_forced: false,
            positions: positions
                .iter()
                .map(|&(at, rotations)| (at, rotations.to_vec()))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn a_tile_that_fits_one_way_plays_without_asking() {
        let (r60, r120, r180) = (Rotation::R60, Rotation::R120, Rotation::R180);
        assert_eq!(auto_rotation(&[r120], false), Some(r120));
        assert_eq!(auto_rotation(&[r120, r180], false), None);
        // The opening tile fits every way round: the first rotation it is.
        assert_eq!(auto_rotation(&[r60, r120, r180], true), Some(r60));
        assert_eq!(auto_rotation(&[], true), None);

        let here = Position::CENTER;
        let there = here.neighbor(Direction::Right);
        assert_eq!(
            auto_move(&info(&[(here, &[r60])]), false),
            Some((here, r60))
        );
        assert_eq!(auto_move(&info(&[(here, &[r60, r180])]), false), None);
        assert_eq!(
            auto_move(&info(&[(here, &[r60, r180])]), true),
            Some((here, r60))
        );
        // Two places to choose from, or none at all: the player decides.
        let two = info(&[(here, &[r60]), (there, &[r60])]);
        assert_eq!(auto_move(&two, false), None);
        assert_eq!(auto_move(&two, true), None);
        assert_eq!(auto_move(&TileMoveInfo::default(), true), None);
    }

    #[test]
    fn the_opening_tile_is_placed_at_the_centre_at_once() {
        let mut page = GamePage::local(seeded(3));
        let (red, rotations) = page.game(|game| {
            let at_centre = &game.hand_info()[0].positions;
            assert_eq!(
                at_centre.keys().copied().collect::<Vec<_>>(),
                [Position::CENTER]
            );
            (game.current_player(), at_centre[&Position::CENTER].clone())
        });
        // Every way round fits: there is nothing to choose, so a click plays.
        assert_eq!(rotations.len(), 6);
        let tile = page.game(|game| game.hand(red)[0]);

        page.act(|ctx| click_tile(ctx, red, 0));
        assert_eq!(
            placed_at(&page, Position::CENTER),
            Some((tile, rotations[0]))
        );
        assert_ne!(page.game(|game| game.current_player()), red);
        assert_eq!(selection(&page), (TurnPhase::SelectTile, None, None));
    }

    #[test]
    fn a_tile_with_one_legal_move_plays_it_at_once() {
        let (game, idx, only) = tile_with_one_move();
        let mover = game.current_player();
        let mut page = GamePage::local(game);

        let board = page.game(|game| game.board().len());
        page.act(|ctx| click_tile(ctx, mover, idx));
        assert_eq!(
            placed_at(&page, only.position),
            Some((only.tile, only.rotation))
        );
        assert_eq!(page.game(|game| game.board().len()), board + 1);
        assert_eq!(selection(&page).1, None);
    }

    #[test]
    fn a_tile_with_several_placements_is_selected() {
        let (game, idx, _) = several_positions();
        let mover = game.current_player();
        let board = game.board().len();
        let mut page = GamePage::local(game);
        page.act(|ctx| ctx.ui_state.message().set("Error: stale".to_string()));

        page.act(|ctx| click_tile(ctx, mover, idx));
        assert_eq!(
            selection(&page),
            (TurnPhase::SelectPosition, Some(idx), None)
        );
        assert_eq!(page.game(|game| game.board().len()), board);
        assert_eq!(page.with(|ctx| ctx.ui_state.message().peek().clone()), "");
    }

    #[test]
    fn clicking_another_tile_moves_the_selection() {
        // Two tiles of the hand to move that fit at several positions.
        let (game, (first, second)) = find_game(|game| {
            let mut choices = game
                .hand_info()
                .iter()
                .enumerate()
                .filter(|(_, info)| info.positions.len() > 1)
                .map(|(idx, _)| idx);
            Some((choices.next()?, choices.next()?))
        });
        let mover = game.current_player();
        let mut page = GamePage::local(game);

        page.act(|ctx| click_tile(ctx, mover, first));
        page.act(|ctx| click_tile(ctx, mover, second));
        assert_eq!(
            selection(&page),
            (TurnPhase::SelectPosition, Some(second), None)
        );
    }

    #[test]
    fn clicking_the_selected_tile_again_puts_it_back() {
        let (game, idx, at) = tile_with_positions(|rotations| rotations.len() > 1);
        let mover = game.current_player();
        let mut page = GamePage::local(game);

        page.act(|ctx| click_tile(ctx, mover, idx));
        page.act(|ctx| click_tile(ctx, mover, idx));
        assert_eq!(selection(&page), (TurnPhase::SelectTile, None, None));

        // From the rotation wheel too: the tile and its position go.
        page.act(|ctx| click_tile(ctx, mover, idx));
        page.act(|ctx| click_position(ctx, at));
        assert_eq!(
            selection(&page),
            (TurnPhase::SelectRotation, Some(idx), Some(at))
        );
        page.act(|ctx| click_tile(ctx, mover, idx));
        assert_eq!(selection(&page), (TurnPhase::SelectTile, None, None));
    }

    #[test]
    fn tiles_cannot_be_clicked_outside_a_human_turn() {
        let (game, idx, only) = tile_with_one_move();
        let mover = game.current_player();
        let page = GamePage::local(game);

        for phase in [TurnPhase::WaitingForAi, TurnPhase::GameOver] {
            // A click lands while the AI plays, or on the results screen.
            let after = page.with(|ctx| {
                ctx.ui_state.phase().set(phase);
                click_tile(ctx, mover, idx);
                *ctx.ui_state.phase().peek()
            });
            assert_eq!(after, phase);
            assert_eq!(selection(&page), (phase, None, None));
            assert_eq!(placed_at(&page, only.position), None);
        }
    }

    #[test]
    fn only_the_player_to_move_can_click_their_tiles() {
        let (game, idx, only) = tile_with_one_move();
        let waiting = game
            .playing_colors()
            .iter()
            .copied()
            .find(|&c| c != game.current_player())
            .unwrap();
        let mut page = GamePage::local(game);

        page.act(|ctx| click_tile(ctx, waiting, idx));
        assert_eq!(selection(&page), (TurnPhase::SelectTile, None, None));
        assert_eq!(placed_at(&page, only.position), None);
    }

    #[test]
    fn online_a_click_plays_only_the_seat_of_this_device() {
        let (game, idx, only) = tile_with_one_move();
        let mover = game.current_player();
        let other = game
            .playing_colors()
            .iter()
            .copied()
            .find(|&c| c != mover)
            .unwrap();

        // Watching, or sitting at another seat: the click does nothing.
        for seat in [None, Some(other)] {
            let (mut page, mut commands) = GamePage::online(game.clone(), seat, EnumMap::default());
            page.act(|ctx| click_tile(ctx, mover, idx));
            assert!(sent(&mut commands).is_empty(), "{seat:?}");
            assert_eq!(selection(&page), (TurnPhase::SelectTile, None, None));
        }

        // From the mover's seat, the move goes to the room, whose next
        // snapshot shows it: the board here stays as it is until then.
        let (mut page, mut commands) = GamePage::online(game, Some(mover), EnumMap::default());
        page.act(|ctx| click_tile(ctx, mover, idx));
        match sent(&mut commands).as_slice() {
            [SessionCommand::PlayMove(placement)] => assert_eq!(*placement, only),
            other => panic!("expected the move, sent {other:?}"),
        }
        assert_eq!(placed_at(&page, only.position), None);
        assert_eq!(selection(&page), (TurnPhase::SelectTile, None, None));
    }

    #[test]
    fn a_hex_taking_one_rotation_plays_it() {
        let (game, idx, at) = tile_with_positions(|rotations| rotations.len() == 1);
        let mover = game.current_player();
        let (tile, rotation) = (
            game.hand(mover)[idx],
            game.hand_info()[idx].positions[&at][0],
        );
        let mut page = GamePage::local(game);

        page.act(|ctx| click_tile(ctx, mover, idx));
        page.act(|ctx| click_position(ctx, at));
        assert_eq!(placed_at(&page, at), Some((tile, rotation)));
    }

    #[test]
    fn a_hex_taking_several_rotations_opens_the_wheel_and_plays_the_pick() {
        let (game, idx, at) = tile_with_positions(|rotations| rotations.len() > 1);
        let mover = game.current_player();
        let tile = game.hand(mover)[idx];
        let picked = game.hand_info()[idx].positions[&at][1];
        let mut page = GamePage::local(game);

        page.act(|ctx| click_tile(ctx, mover, idx));
        page.act(|ctx| click_position(ctx, at));
        assert_eq!(
            selection(&page),
            (TurnPhase::SelectRotation, Some(idx), Some(at))
        );
        assert_eq!(placed_at(&page, at), None);

        page.act(|ctx| pick_rotation(ctx, picked));
        assert_eq!(placed_at(&page, at), Some((tile, picked)));
    }

    #[test]
    fn hexes_the_tile_does_not_fit_and_clicks_before_a_tile_do_nothing() {
        let (game, idx, at) = several_positions();
        let mover = game.current_player();
        let nowhere = (0..10).fold(at, |hex, _| hex.neighbor(Direction::Right));
        let mut page = GamePage::local(game);

        // No tile selected yet.
        page.act(|ctx| click_position(ctx, at));
        assert_eq!(selection(&page), (TurnPhase::SelectTile, None, None));
        assert_eq!(placed_at(&page, at), None);

        page.act(|ctx| click_tile(ctx, mover, idx));
        page.act(|ctx| click_position(ctx, nowhere));
        assert_eq!(
            selection(&page),
            (TurnPhase::SelectPosition, Some(idx), None)
        );
        // Nor is a rotation picked without a position.
        page.act(|ctx| pick_rotation(ctx, Rotation::None));
        assert_eq!(page.game(|game| game.current_player()), mover);
    }

    #[test]
    fn a_tile_the_hand_does_not_hold_plays_nothing() {
        let mut page = GamePage::local(seeded(3));
        page.act(|ctx| execute_move(ctx, 6, Position::CENTER, Rotation::None));
        assert!(page.game(|game| game.board().is_empty()));
    }

    #[test]
    fn ending_a_local_game_ends_it_here() {
        let mut page = GamePage::local(seeded(3));
        page.act(end_game);
        let (phase, message) = page.with(|ctx| {
            (
                *ctx.ui_state.phase().peek(),
                ctx.ui_state.message().peek().clone(),
            )
        });
        assert_eq!(phase, TurnPhase::GameOver);
        assert_eq!(message, "Game ended early.");
    }

    #[test]
    fn ending_an_online_game_asks_the_room() {
        let (mut page, mut commands) =
            GamePage::online(seeded(3), Some(Color::Red), EnumMap::default());
        page.act(|mut ctx| ctx.ui_state.select_tile(0));
        page.act(end_game);
        assert!(matches!(
            sent(&mut commands).as_slice(),
            [SessionCommand::EndGame]
        ));
        // The room's `GameEnded` ends it here, not the click.
        assert_eq!(selection(&page).0, TurnPhase::SelectPosition);
    }
}
