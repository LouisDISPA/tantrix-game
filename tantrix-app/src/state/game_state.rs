use std::ops::Deref;

use dioxus::prelude::*;
use tantrix_game::{EnumMap, Game, MoveError, Placement, PlayedMove, TileMoveInfo};
use tantrix_proto::GameSnapshot;

use super::config::GameConfig;
use super::game_driver::GameDriver;
use super::ui_state::UiState;

/// The single context shared by every component of the game screens, local
/// or online. Built once per game page by `hooks::use_game_ctx`.
///
/// * `game_state` — the atomic game snapshot; components read it wholesale
/// * `ui_state` — per-field reactive UI state (zoom, hover, selection, …)
/// * `driver` — who plays: every seat on this device, or an online room
///   this device sends its seat's moves to (see [`GameDriver`])
#[derive(Clone, Copy)]
pub struct GameCtx {
    pub game_state: Signal<GameState>,
    pub ui_state: Store<UiState>,
    pub driver: GameDriver,
}

/// The game as this device sees it. [`Game`] is the single source of truth
/// for the rules and the state; `GameState` only adds `hand_info`, the
/// current player's legal moves grouped per hand tile, kept in step with
/// every change so components never regroup them while rendering.
///
/// Reads go straight to the `Game` (`GameState` derefs to it); changes go
/// through `GameState` so the cache stays in sync. Treated as one atomic
/// unit — every change touches the board, hands, turn and cache together —
/// so providers wrap it in a plain `Signal<GameState>`.
#[derive(Debug, Clone)]
pub struct GameState {
    game: Game,
    hand_info: Vec<TileMoveInfo>,
}

impl Deref for GameState {
    type Target = Game;

    fn deref(&self) -> &Game {
        &self.game
    }
}

impl GameState {
    /// Deal a new game. A seeded config always deals the same bag and turn
    /// order; either way the game keeps its seed, so it can be replayed.
    pub fn new(config: &GameConfig) -> Self {
        let players = EnumMap::from_fn(|color| config.player_for(color));
        let game = match config.seed {
            Some(seed) => Game::from_players_seeded(players, seed),
            None => Game::from_players(players),
        };
        Self::from_game(game.expect("GameConfig always creates at least two active players"))
    }

    /// Rebuild from an authoritative snapshot received over the wire. The
    /// bag's contents are stubbed (only its count matters for the UI), and
    /// each playing color is human-owned.
    pub fn from_snapshot(snap: &GameSnapshot) -> Self {
        let game = Game::from_snapshot(
            snap.board.clone(),
            snap.hands.clone(),
            snap.bag_count,
            snap.current_player,
            snap.playing_colors.iter().copied(),
            snap.turn_step,
        )
        .expect("authoritative snapshots always include the current player");
        Self::from_game(game)
    }

    fn from_game(game: Game) -> Self {
        let hand_info = game.moves_by_tile();
        Self { game, hand_info }
    }

    /// The current player's legal moves per hand tile, in hand order.
    pub fn hand_info(&self) -> &[TileMoveInfo] {
        &self.hand_info
    }

    /// Play `placement` for the current player (see [`Game::play_move`]).
    /// They keep the turn while forced spaces remain to fill after their
    /// free move.
    pub fn play_move(&mut self, placement: Placement) -> Result<PlayedMove, MoveError> {
        let played = self.game.play_move(placement)?;
        self.refresh_hand_info();
        Ok(played)
    }

    /// Pass the turn over players who cannot move; `true` when nobody can
    /// (see [`Game::skip_stuck_players`]). When the current player can move,
    /// nothing changes and `hand_info` is kept as it is.
    pub fn skip_stuck_players(&mut self) -> bool {
        if !self.game.is_stuck() {
            return false;
        }
        let stalled = self.game.skip_stuck_players();
        self.refresh_hand_info();
        stalled
    }

    fn refresh_hand_info(&mut self) {
        self.hand_info = self.game.moves_by_tile();
    }
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Board, Color, EnumMap, TurnStep};
    use tantrix_proto::GameSnapshot;

    use super::GameState;
    use crate::state::config::GameConfig;
    use crate::state::test_games::{fixture, seeded, Fixture, FixtureMove};

    #[test]
    fn seeded_config_deals_the_same_game() {
        let a = seeded(7);
        let b = seeded(7);
        assert_eq!(a.hands(), b.hands());
        assert_eq!(a.playing_colors(), b.playing_colors());
        assert_eq!(a.seed(), Some(7));
        assert!(GameState::new(&GameConfig::new(None)).seed().is_some());
    }

    #[test]
    fn e2e_fixture_keeps_turn_to_fill_forced_space_after_free_move() {
        let fixture = fixture();
        let mut game = seeded(fixture.seed);

        for (i, step) in fixture.moves.iter().enumerate() {
            assert_eq!(game.current_player(), step.player, "move {i}");
            let played = game
                .play_move(step.placement())
                .unwrap_or_else(|e| panic!("move {i}: {e}"));
            // The board decides whether the move filled a forced space.
            assert_eq!(played.m.forced, step.forced, "move {i}");
            let placed = game
                .board()
                .iter()
                .find(|m| m.position == step.position)
                .unwrap();
            assert_eq!(*placed, played.m, "move {i}");
            assert_eq!(game.current_player(), step.next, "move {i}");
        }

        // The fixture shows the rule: after a free move the same player fills
        // the forced space, then the turn passes.
        assert!(fixture.moves.windows(2).any(|w| {
            !w[0].forced && w[0].next == w[0].player && w[1].forced && w[1].player == w[0].player
        }));
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn hand_info_follows_every_change() {
        let fixture = fixture();
        let mut game = seeded(fixture.seed);
        assert_eq!(game.hand_info(), game.moves_by_tile());
        for step in &fixture.moves {
            game.play_move(step.placement()).unwrap();
            assert_eq!(game.hand_info(), game.moves_by_tile());
        }
    }

    #[test]
    fn a_player_who_can_move_is_not_skipped() {
        let mut game = seeded(7);
        let (player, hand_info) = (game.current_player(), game.hand_info().to_vec());
        assert!(!game.is_stuck());

        assert!(!game.skip_stuck_players());
        assert_eq!(game.current_player(), player);
        assert_eq!(game.hand_info(), hand_info);
        assert!(game.history().is_empty());
    }

    #[test]
    fn a_round_where_nobody_can_move_stalls_the_game() {
        // Tiles are left in the bag, but no one holds any: nobody can move.
        let snapshot = GameSnapshot {
            board: Board::default(),
            hands: EnumMap::default(),
            bag_count: 10,
            current_player: Color::Blue,
            turn_step: TurnStep::BeforeFreeMove,
            playing_colors: vec![Color::Red, Color::Blue],
            seats: EnumMap::default(),
            participants: Vec::new(),
        };
        let mut game = GameState::from_snapshot(&snapshot);
        assert!(!game.is_game_over());

        assert!(game.skip_stuck_players());
        assert_eq!(game.current_player(), Color::Blue);
        assert!(game.hand_info().is_empty());
    }

    /// Prints a fresh fixture after a change to dealing or move order:
    /// `cargo test -p tantrix-app print_forced_after_free_move_fixture -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn print_forced_after_free_move_fixture() {
        let fixture = (0..500)
            .filter_map(forced_after_free_move_game)
            .min_by_key(|f| f.moves.len())
            .expect("a seed reaches a forced space after a free move");
        println!("{}", serde_json::to_string_pretty(&fixture).unwrap());
    }

    /// Plays the first legal move until a free move leaves its player a forced
    /// space, then until that player's turn ends.
    fn forced_after_free_move_game(seed: u64) -> Option<Fixture> {
        let mut game = seeded(seed);
        let mut moves = Vec::new();
        let mut keeper = None;
        while moves.len() < 20 {
            let player = game.current_player();
            let m = *game.legal_moves().first()?;
            game.play_move(m.placement()).ok()?;
            let next = game.current_player();
            moves.push(FixtureMove {
                player,
                tile: m.tile,
                position: m.position,
                rotation: m.rotation,
                forced: m.forced,
                next,
            });
            if game.turn_step() == TurnStep::AfterFreeMove {
                keeper = Some(player);
            } else if keeper.is_some_and(|p| p != next) {
                return Some(Fixture { seed, moves });
            }
        }
        None
    }
}
