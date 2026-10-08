//! The randomness of the AIs that sample: their own seeded generator
//! ([`TantrixAi::with_seed`](tantrix_game::TantrixAi::with_seed)), or the
//! thread's.

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

    /// A generator for one decision, seeded from this one: a seeded AI
    /// makes the same decisions from the same seed, and holds no lock while
    /// it thinks.
    pub(crate) fn fork(&self) -> Xoshiro256PlusPlus {
        match &self.seeded {
            Some(rng) => {
                let seed = rng
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .next_u64();
                Xoshiro256PlusPlus::seed_from_u64(seed)
            }
            None => Xoshiro256PlusPlus::from_rng(&mut rand::rng()),
        }
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
