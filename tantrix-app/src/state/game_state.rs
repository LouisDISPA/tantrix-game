use std::collections::BTreeMap;

use dioxus::prelude::*;
use rand::rngs::Xoshiro256PlusPlus;
use rand::{Rng, SeedableRng};
use tantrix_game::{
    play_move_parts, Bag, Board, Color, EnumMap, Hand, Move, MoveError, Player, PlayerScore,
    Position, Rotation, TurnTracker, TILES_IN_HAND,
};
use tantrix_proto::GameSnapshot;

use super::config::GameConfig;
use super::online_state::CommandSender;
use super::ui_state::{Screen, UiState};

/// The single context shared across every component in the local-game page.
///
/// * `screen` — which top-level screen is showing (Setup / Playing)
/// * `game_state` — the atomic game snapshot; components read it wholesale
/// * `ui_state` — per-field reactive UI state (zoom, hover, selection, …)
/// * `online` — if set, `actions::execute_move` forwards the move to the
///   server instead of mutating the local `game_state`. Local games leave
///   this as `None`.
/// * `my_color` — if `Some(c)`, only this color's hand is interactive on
///   this device. Online games set this from the lobby seat; local games
///   leave it `None` so every current-turn hand is clickable.
/// * `player_names` — optional nicknames per color, shown beside the color
///   in each hand's header. Empty for local games.
#[derive(Clone, Copy)]
pub struct GameCtx {
    pub screen: Signal<Screen>,
    pub game_state: Signal<GameState>,
    pub ui_state: Store<UiState>,
    pub online: Signal<Option<CommandSender>>,
    pub my_color: Signal<Option<Color>>,
    pub player_names: Signal<EnumMap<Color, Option<String>>>,
}

#[derive(Debug, Clone)]
pub struct TileMoveInfo {
    pub has_forced: bool,
    pub positions: BTreeMap<Position, Vec<Rotation>>,
}

impl TileMoveInfo {
    pub fn has_moves(&self) -> bool {
        !self.positions.is_empty()
    }
}

/// Core game data. `GameState` is treated as one atomic unit — every mutation
/// (`play_move`, `skip_turn`, …) updates multiple fields together, so there's
/// nothing to gain from per-field reactivity. Providers wrap this in a plain
/// `Signal<GameState>`.
#[derive(Debug, Clone)]
pub struct GameState {
    pub board: Board,
    pub bag: Bag,
    pub hands: EnumMap<Color, Hand>,
    pub players: EnumMap<Color, Player>,
    pub turn_tracker: TurnTracker,
    pub hand_info: Vec<TileMoveInfo>,
}

impl GameState {
    /// Deal a new game. A seeded config always deals the same bag and turn order.
    pub fn new(config: &GameConfig) -> Self {
        match config.seed {
            Some(seed) => Self::deal(config, &mut Xoshiro256PlusPlus::seed_from_u64(seed)),
            None => Self::deal(config, &mut rand::rng()),
        }
    }

    fn deal<R: Rng + ?Sized>(config: &GameConfig, rng: &mut R) -> Self {
        let players = EnumMap::from_fn(|color| config.player_for(color));

        let mut bag = Bag::new_with_rng(rng);
        let turn_tracker = TurnTracker::new_with_rng(&players, rng)
            .expect("GameConfig always creates at least two active players");

        let mut hands: EnumMap<Color, Hand> = Default::default();
        for &color in turn_tracker.playing_colors() {
            hands[color].deal(&mut bag, TILES_IN_HAND);
        }

        let mut state = GameState {
            board: Board::default(),
            bag,
            hands,
            players,
            turn_tracker,
            hand_info: Vec::new(),
        };
        state.refresh_hand_info();
        state
    }

    /// Rebuild from an authoritative snapshot received over the wire. The
    /// bag's contents are stubbed (only its count matters for the UI), and
    /// each seated color is mapped to `Player::Someone`.
    pub fn from_snapshot(snap: &GameSnapshot) -> Self {
        let players: EnumMap<Color, Player> = EnumMap::from_fn(|c| {
            if snap.seats[c].is_some() {
                Player::Someone
            } else {
                Player::None
            }
        });

        let mut state = GameState {
            board: snap.board.clone(),
            bag: Bag::with_count(snap.bag_count),
            hands: snap.hands.clone(),
            players,
            turn_tracker: TurnTracker::from_parts(
                snap.current_player,
                snap.playing_colors.iter().copied(),
                snap.turn_step,
            )
            .expect("authoritative snapshots always include the current player"),
            hand_info: Vec::new(),
        };
        state.refresh_hand_info();
        state
    }

    /// Legal moves for the current player at their step of the turn.
    fn current_moves(&self) -> Vec<Move> {
        let current = self.turn_tracker.current();
        self.board.possible_moves(
            &self.hands[current],
            self.bag.is_empty(),
            self.turn_tracker.step(),
        )
    }

    pub fn refresh_hand_info(&mut self) {
        let current = self.turn_tracker.current();
        let all_moves = self.current_moves();

        self.hand_info = self.hands[current]
            .iter()
            .map(|tile| {
                let tile_moves: Vec<&Move> = all_moves.iter().filter(|m| m.tile == *tile).collect();
                let has_forced = tile_moves.iter().any(|m| m.forced);

                let mut positions: BTreeMap<Position, Vec<Rotation>> = BTreeMap::new();
                for m in &tile_moves {
                    positions.entry(m.position).or_default().push(m.rotation);
                }

                TileMoveInfo {
                    has_forced,
                    positions,
                }
            })
            .collect();
    }

    /// Play `m` for the current player. They keep the turn while forced
    /// spaces remain to fill after their free move.
    pub fn play_move(&mut self, m: Move) -> Result<(), MoveError> {
        play_move_parts(
            &mut self.board,
            &mut self.hands,
            &mut self.bag,
            &mut self.turn_tracker,
            m,
        )?;
        self.refresh_hand_info();
        Ok(())
    }

    pub fn skip_turn(&mut self) {
        self.turn_tracker.next();
        self.refresh_hand_info();
    }

    /// Skip past any players with no valid moves. Stops at a player who can act or game over.
    pub fn skip_stuck_players(&mut self) {
        let num_players = self.turn_tracker.playing_colors().len();
        for _ in 0..num_players {
            if self.is_game_over() || self.has_any_moves() {
                return;
            }
            self.turn_tracker.next();
            self.refresh_hand_info();
        }
    }

    /// If the current player is AI, compute its chosen move.
    pub fn ai_choose(&self) -> Option<Move> {
        if self.is_game_over() {
            return None;
        }
        let current = self.turn_tracker.current();
        let possible = self.current_moves();
        if possible.is_empty() {
            return None;
        }
        if let Player::Ai(ref ai) = self.players[current] {
            Some(ai.choose_move(
                &self.board,
                &self.hands,
                current,
                self.turn_tracker.playing_colors(),
                &possible,
            ))
        } else {
            None
        }
    }

    pub fn is_game_over(&self) -> bool {
        self.bag.is_empty() && self.hands.values().all(|h| h.is_empty())
    }

    pub fn has_any_moves(&self) -> bool {
        self.hand_info.iter().any(|info| info.has_moves())
    }
}

/// Determine the winner(s) of a finished game from player scores.
/// Returns the list of players tied for the highest score and whether it's a tie.
pub fn determine_winners(
    active_players: &[Color],
    scores: &EnumMap<Color, PlayerScore>,
) -> (Vec<Color>, bool) {
    let max_score = active_players
        .iter()
        .map(|&c| scores[c].score)
        .max()
        .unwrap_or(0);
    let winners: Vec<Color> = active_players
        .iter()
        .copied()
        .filter(|&c| scores[c].score == max_score)
        .collect();
    let is_tie = winners.len() > 1;
    (winners, is_tie)
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use tantrix_game::{Color, Move, Position, Rotation, Tile, TurnStep};

    use super::GameState;
    use crate::state::config::GameConfig;

    /// The seeded game that `e2e/tests/forced-after-free-move.spec.ts` plays
    /// in the browser: a free move creates a forced space its player can fill,
    /// so they keep the turn to fill it.
    const FIXTURE: &str = include_str!("../../../e2e/fixtures/forced-after-free-move.json");

    #[derive(Debug, Serialize, Deserialize)]
    struct Fixture {
        seed: u64,
        moves: Vec<FixtureMove>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct FixtureMove {
        player: Color,
        tile: Tile,
        position: Position,
        rotation: Rotation,
        forced: bool,
        /// Whose turn it is after this move.
        next: Color,
    }

    fn seeded(seed: u64) -> GameState {
        GameState::new(&GameConfig::new(Some(seed)))
    }

    #[test]
    fn seeded_config_deals_the_same_game() {
        let a = seeded(7);
        let b = seeded(7);
        assert_eq!(a.hands, b.hands);
        assert_eq!(
            a.turn_tracker.playing_colors(),
            b.turn_tracker.playing_colors()
        );
    }

    #[test]
    fn e2e_fixture_keeps_turn_to_fill_forced_space_after_free_move() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).unwrap();
        let mut game = seeded(fixture.seed);

        for (i, step) in fixture.moves.iter().enumerate() {
            assert_eq!(game.turn_tracker.current(), step.player, "move {i}");
            game.play_move(Move {
                tile: step.tile,
                position: step.position,
                rotation: step.rotation,
                forced: step.forced,
            })
            .unwrap_or_else(|e| panic!("move {i}: {e}"));
            let placed = game
                .board
                .iter()
                .find(|m| m.position == step.position)
                .unwrap();
            assert_eq!(placed.forced, step.forced, "move {i}");
            assert_eq!(game.turn_tracker.current(), step.next, "move {i}");
        }

        // The fixture shows the rule: after a free move the same player fills
        // the forced space, then the turn passes.
        assert!(fixture.moves.windows(2).any(|w| {
            !w[0].forced && w[0].next == w[0].player && w[1].forced && w[1].player == w[0].player
        }));
        assert_eq!(game.turn_tracker.step(), TurnStep::BeforeFreeMove);
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
            let player = game.turn_tracker.current();
            let m = *game.current_moves().first()?;
            game.play_move(m).ok()?;
            let next = game.turn_tracker.current();
            moves.push(FixtureMove {
                player,
                tile: m.tile,
                position: m.position,
                rotation: m.rotation,
                forced: m.forced,
                next,
            });
            if game.turn_tracker.step() == TurnStep::AfterFreeMove {
                keeper = Some(player);
            } else if keeper.is_some_and(|p| p != next) {
                return Some(Fixture { seed, moves });
            }
        }
        None
    }
}
