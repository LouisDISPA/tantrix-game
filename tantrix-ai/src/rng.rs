//! The randomness of the AIs that pick or sample at random: their own
//! seeded generator ([`TantrixAi::with_seed`](tantrix_game::TantrixAi::with_seed)),
//! or OS entropy.

use std::sync::{Mutex, PoisonError};

use rand::rngs::Xoshiro256PlusPlus;
use rand::{Rng, SeedableRng};

#[derive(Default)]
pub(crate) struct AiRng {
    /// `None` draws from the thread's RNG. A seeded AI owns its RNG, behind
    /// a `Mutex` since choosing a move takes `&self` and AIs are `Sync`.
    seeded: Option<Mutex<Xoshiro256PlusPlus>>,
}

impl AiRng {
    pub(crate) fn seeded(seed: u64) -> Self {
        Self {
            seeded: Some(Mutex::new(Xoshiro256PlusPlus::seed_from_u64(seed))),
        }
    }

    /// Run `f` with this generator: a seeded AI's own, so it advances by
    /// exactly what `f` draws, or a fresh one from OS entropy.
    #[expect(
        clippy::disallowed_methods,
        reason = "an unseeded AI draws from OS entropy"
    )]
    pub(crate) fn with<T>(&self, f: impl FnOnce(&mut Xoshiro256PlusPlus) -> T) -> T {
        match &self.seeded {
            // A panic mid-draw leaves the generator usable; its state is valid.
            Some(rng) => f(&mut rng.lock().unwrap_or_else(PoisonError::into_inner)),
            None => f(&mut Xoshiro256PlusPlus::from_rng(&mut rand::rng())),
        }
    }

    /// A generator for one decision, seeded from this one: a seeded AI
    /// makes the same decisions from the same seed, and holds no lock while
    /// it thinks.
    pub(crate) fn fork(&self) -> Xoshiro256PlusPlus {
        self.with(|rng| Xoshiro256PlusPlus::seed_from_u64(rng.next_u64()))
    }
}

impl Clone for AiRng {
    /// A seeded copy carries on from this one's state.
    fn clone(&self) -> Self {
        Self {
            seeded: self
                .seeded
                .as_ref()
                .map(|rng| Mutex::new(rng.lock().unwrap_or_else(PoisonError::into_inner).clone())),
        }
    }
}
