//! The shared draw pile.

use arrayvec::ArrayVec;
use rand::prelude::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::tiles::NUMBER_OF_TILE;
use crate::Tile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bag {
    tiles: ArrayVec<Tile, NUMBER_OF_TILE>,
}

impl Bag {
    pub fn new() -> Self {
        Self::new_with_rng(&mut rand::rng())
    }

    /// A full bag shuffled by `rng`; a seeded `rng` always gives the same draws.
    pub fn new_with_rng<R: Rng + ?Sized>(rng: &mut R) -> Self {
        let mut tiles: ArrayVec<Tile, NUMBER_OF_TILE> = Default::default();
        for t in enum_iterator::all::<Tile>() {
            tiles.push(t);
        }
        tiles.shuffle(rng);
        Bag { tiles }
    }

    pub fn draw(&mut self) -> Option<Tile> {
        self.tiles.pop()
    }

    pub fn count(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Build a bag with `n` placeholder tiles. The contents are arbitrary —
    /// this constructor exists so remote clients can mirror the host's
    /// `bag_count` without seeing the real draw order.
    pub fn with_count(n: usize) -> Self {
        let mut tiles: ArrayVec<Tile, NUMBER_OF_TILE> = Default::default();
        for _ in 0..n.min(NUMBER_OF_TILE) {
            tiles.push(Tile::RYYBRB);
        }
        Bag { tiles }
    }
}

impl Default for Bag {
    fn default() -> Self {
        Self::new()
    }
}
