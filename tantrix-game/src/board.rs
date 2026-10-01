//! Board storage: placed moves sorted by position, lookups, the contour of
//! empty positions around them, and neighbor counts.
//!
//! The rules that decide what may be placed live in `rules.rs`, and
//! scoring in `scoring.rs`; both extend [`Board`] with more methods.

use std::collections::BTreeSet;
use std::ops::Deref;

use arrayvec::ArrayVec;
use serde::{Deserialize, Serialize};

use crate::tiles::NUMBER_OF_TILE;
use crate::{Move, Position};

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    moves: ArrayVec<Move, NUMBER_OF_TILE>,
}

impl Deref for Board {
    type Target = [Move];

    fn deref(&self) -> &Self::Target {
        &self.moves
    }
}

impl Board {
    pub fn is_available(&self, pos: &Position) -> bool {
        self.moves
            .binary_search_by(|m| m.position.cmp(pos))
            .is_err()
    }

    pub fn insert_move(&mut self, m: Move) -> Result<(), String> {
        let index = match self.moves.binary_search_by(|i| i.position.cmp(&m.position)) {
            Ok(_) => return Err("Position unavailable".to_string()),
            Err(i) => i,
        };
        self.moves.try_insert(index, m).map_err(|e| e.to_string())
    }

    /// Take the tile at `pos` off the board: undoes [`Self::insert_move`].
    pub(crate) fn remove_move(&mut self, pos: &Position) -> Option<Move> {
        let index = self.moves.binary_search_by(|m| m.position.cmp(pos)).ok()?;
        Some(self.moves.remove(index))
    }

    pub(crate) fn neighbor_count(&self, pos: &Position) -> u8 {
        pos.around().filter(|(p, _)| !self.is_available(p)).count() as u8
    }

    pub(crate) fn get_move(&self, pos: &Position) -> Option<Move> {
        self.index_of(pos).map(|index| self.moves[index])
    }

    /// Where the tile at `pos` is in board order, if there is one.
    pub(crate) fn index_of(&self, pos: &Position) -> Option<usize> {
        self.moves.binary_search_by(|m| m.position.cmp(pos)).ok()
    }

    pub fn contour_positions(&self) -> BTreeSet<Position> {
        if self.is_empty() {
            BTreeSet::from([Position::CENTER])
        } else {
            self.moves
                .iter()
                .flat_map(|m| m.position.around())
                .filter(|&(pos, _)| self.is_available(&pos))
                .map(|(pos, _)| pos)
                .collect()
        }
    }
}
