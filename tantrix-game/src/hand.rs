//! A player's tiles, face up, refilled from the bag after every play.

use std::ops::Deref;

use arrayvec::ArrayVec;
use serde::{Deserialize, Serialize};

use crate::{Bag, Tile};

pub const TILES_IN_HAND: usize = 6;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hand {
    tiles: ArrayVec<Tile, TILES_IN_HAND>,
}

impl Hand {
    /// Remove a tile from hand and draw a replacement from the bag.
    /// Returns the index where the tile was removed.
    pub fn play_tile(&mut self, tile: Tile, bag: &mut Bag) -> usize {
        let index = self
            .tiles
            .iter()
            .position(|t| *t == tile)
            .expect("Can't play tile not in hand");
        self.tiles.remove(index);
        // Draw replacements to fill back up
        let to_draw = TILES_IN_HAND - self.tiles.len();
        for i in 0..to_draw.min(bag.count()) {
            let new_tile = bag.draw().unwrap();
            let idx = (index + i).min(self.tiles.len());
            self.tiles.insert(idx, new_tile);
        }
        index
    }

    /// Deal `count` tiles from the bag into this hand.
    pub fn deal(&mut self, bag: &mut Bag, count: usize) {
        for _ in 0..count {
            if let Some(tile) = bag.draw() {
                self.tiles.push(tile);
            }
        }
    }
}

impl Deref for Hand {
    type Target = [Tile];

    fn deref(&self) -> &Self::Target {
        &self.tiles
    }
}
