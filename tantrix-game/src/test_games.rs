//! Seeded games for tests that compare two ways of computing the same
//! thing, or check an invariant, over many game states.

use rand::rngs::Xoshiro256PlusPlus;
use rand::{Rng, SeedableRng};

use crate::{Board, Game, GameStatus, Player};

/// Plays `games` seeded games of two to four players, each move picked at
/// random (seeded) among the legal ones, and calls `visit` on every state
/// on the way, the deal and the end included.
pub(crate) fn for_each_seeded_state(games: u64, visit: impl FnMut(&Game)) {
    for_each_seeded_state_after(&Board::default(), games, visit);
}

/// [`for_each_seeded_state`], with `opening` already on the board when each
/// game starts ([`Game::with_opening`]): seeded play on from a hand-built
/// position that random play would hardly reach.
pub(crate) fn for_each_seeded_state_after(
    opening: &Board,
    games: u64,
    mut visit: impl FnMut(&Game),
) {
    for seed in 0..games {
        let player = |color_index: u64| {
            if color_index < 2 + seed % 3 {
                Player::Someone
            } else {
                Player::None
            }
        };
        let mut game = Game::new_seeded(player(0), player(1), player(2), player(3), seed).unwrap();
        if !opening.is_empty() {
            game = game.with_opening(opening.clone());
        }
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        loop {
            visit(&game);
            if game.status() != GameStatus::Playing {
                break;
            }
            if game.legal_moves().is_empty() {
                game.skip_stuck_players();
                continue;
            }
            let moves = game.legal_moves();
            let m = moves[(rng.next_u64() % moves.len() as u64) as usize];
            game.play_move(m.placement()).unwrap();
        }
    }
}
