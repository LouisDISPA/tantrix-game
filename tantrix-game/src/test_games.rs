//! Seeded games for tests that compare two ways of computing the same
//! thing over many game states.

use rand::rngs::Xoshiro256PlusPlus;
use rand::{Rng, SeedableRng};

use crate::{Game, Player};

/// Plays `games` seeded games of two to four players, each move picked at
/// random (seeded) among the legal ones, and calls `visit` on every state
/// on the way, the deal and the end included.
pub(crate) fn for_each_seeded_state(games: u64, mut visit: impl FnMut(&Game)) {
    for seed in 0..games {
        let player = |color_index: u64| {
            if color_index < 2 + seed % 3 {
                Player::Someone
            } else {
                Player::None
            }
        };
        let mut game = Game::new_seeded(player(0), player(1), player(2), player(3), seed).unwrap();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        loop {
            visit(&game);
            if game.is_game_over() {
                break;
            }
            if game.legal_moves().is_empty() {
                if game.skip_stuck_players() {
                    break;
                }
                continue;
            }
            let moves = game.legal_moves();
            let m = moves[(rng.next_u64() % moves.len() as u64) as usize];
            game.play_move(m.placement()).unwrap();
        }
    }
}
