use std::sync::{Mutex, MutexGuard, PoisonError};

use rand::prelude::IndexedRandom;
use rand::rngs::Xoshiro256PlusPlus;
use rand::SeedableRng;
use tantrix_game::{AiView, Move, TantrixAi};

/// Plays a random legal move. [`RandomAi::seeded`] makes its choices
/// reproducible.
#[derive(Default)]
pub struct RandomAi {
    /// `None` draws from the thread's RNG. A seeded AI owns its RNG, behind
    /// a `Mutex` since choosing a move takes `&self` and AIs are `Sync`.
    rng: Option<Mutex<Xoshiro256PlusPlus>>,
}

impl RandomAi {
    /// Draws its choices from the thread's RNG.
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes the same choices every time from the same seed, given the same
    /// moves to choose from.
    pub fn seeded(seed: u64) -> Self {
        Self {
            rng: Some(Mutex::new(Xoshiro256PlusPlus::seed_from_u64(seed))),
        }
    }
}

fn lock(rng: &Mutex<Xoshiro256PlusPlus>) -> MutexGuard<'_, Xoshiro256PlusPlus> {
    // A panic mid-choice leaves the RNG usable; its state is still valid.
    rng.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Clone for RandomAi {
    /// A seeded copy carries on from this one's RNG state: both make the same
    /// choices from here on.
    fn clone(&self) -> Self {
        Self {
            rng: self.rng.as_ref().map(|rng| Mutex::new(lock(rng).clone())),
        }
    }
}

impl TantrixAi for RandomAi {
    fn name(&self) -> &str {
        "Random"
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn with_seed(&self, seed: u64) -> Box<dyn TantrixAi> {
        Box::new(Self::seeded(seed))
    }

    fn choose_move(&self, view: &AiView) -> Move {
        let chosen = match &self.rng {
            Some(rng) => view.legal_moves.choose(&mut *lock(rng)),
            None => view.legal_moves.choose(&mut rand::rng()),
        };
        *chosen.expect("possible_moves must not be empty")
    }
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Game, Player, TantrixAi};

    use super::RandomAi;

    /// The moves `ai` picks from the same position, `count` times over.
    fn choices(ai: &dyn TantrixAi, count: usize) -> Vec<tantrix_game::Move> {
        let game = Game::new_seeded(
            Player::Someone,
            Player::Someone,
            Player::None,
            Player::None,
            0,
        )
        .unwrap();
        (0..count)
            .map(|_| ai.choose_move(&game.ai_view()))
            .collect()
    }

    #[test]
    fn a_seeded_ai_repeats_its_choices() {
        let picks = choices(&RandomAi::seeded(3), 20);
        assert_eq!(picks, choices(&RandomAi::seeded(3), 20));
        // Not the same move every time.
        assert!(picks.iter().any(|m| *m != picks[0]));
    }

    #[test]
    fn a_copy_carries_on_from_the_same_state() {
        let ai = RandomAi::seeded(3);
        choices(&ai, 5);
        let copy = ai.clone_box();
        assert_eq!(choices(&ai, 10), choices(copy.as_ref(), 10));
        assert_eq!(
            choices(RandomAi::new().with_seed(3).as_ref(), 20),
            choices(&RandomAi::seeded(3), 20)
        );
    }
}
