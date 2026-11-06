use rand::prelude::IndexedRandom;
use tantrix_game::{AiView, Move, TantrixAi};

use crate::rng::AiRng;

/// Plays a random legal move. [`RandomAi::seeded`] makes its choices
/// reproducible; a seeded copy carries on from the same RNG state, so both
/// make the same choices from there.
#[derive(Default, Clone)]
pub struct RandomAi {
    rng: AiRng,
}

impl RandomAi {
    /// Draws its choices from OS entropy.
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes the same choices every time from the same seed, given the same
    /// moves to choose from.
    pub fn seeded(seed: u64) -> Self {
        Self {
            rng: AiRng::seeded(seed),
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
        let chosen = self.rng.with(|rng| view.legal_moves.choose(rng).copied());
        chosen.expect("possible_moves must not be empty")
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
