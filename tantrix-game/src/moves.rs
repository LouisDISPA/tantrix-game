//! A tile placed (or to be placed) on the board.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Color, Direction, Position, Rotation, Tile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Move {
    pub tile: Tile,
    pub position: Position,
    pub rotation: Rotation,
    pub forced: bool,
}

impl Move {
    pub fn color(&self, dir: Direction) -> Color {
        self.tile.color(dir - self.rotation)
    }
}

/// A move as [`Game::play_move`](crate::Game::play_move) placed it: the
/// board's own copy (its `forced` flag is the board's, not the caller's),
/// and where the tile was in the hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayedMove {
    pub m: Move,
    pub hand_index: usize,
}

/// The legal moves of one hand tile, grouped by position.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TileMoveInfo {
    /// The tile fills a forced space; it then has only forced moves.
    pub has_forced: bool,
    /// Each position the tile fits, with the rotations that fit there.
    pub positions: BTreeMap<Position, Vec<Rotation>>,
}

impl TileMoveInfo {
    /// Group the moves of `tile` among `moves`, keeping their order.
    pub(crate) fn of(tile: Tile, moves: &[Move]) -> Self {
        let mut info = Self::default();
        for m in moves.iter().filter(|m| m.tile == tile) {
            info.has_forced |= m.forced;
            info.positions
                .entry(m.position)
                .or_default()
                .push(m.rotation);
        }
        info
    }

    pub fn has_moves(&self) -> bool {
        !self.positions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Color, Direction, Move, Position, Rotation, Tile};

    #[test]
    fn move_colors_follow_the_rotation() {
        let m = Move {
            tile: Tile::BYYBRR,
            position: Position::CENTER,
            rotation: Rotation::None,
            forced: false,
        };
        assert_eq!(m.color(Direction::Left), Color::Blue);
        assert_eq!(m.color(Direction::TopLeft), Color::Yellow);
        assert_eq!(m.color(Direction::TopRight), Color::Yellow);
        assert_eq!(m.color(Direction::Right), Color::Blue);
        assert_eq!(m.color(Direction::BottomRight), Color::Red);
        assert_eq!(m.color(Direction::BottomLeft), Color::Red);

        let m = Move {
            tile: Tile::BYYBRR,
            position: Position::CENTER,
            rotation: Rotation::R120,
            forced: false,
        };
        assert_eq!(m.color(Direction::TopRight), Color::Blue);
        assert_eq!(m.color(Direction::Right), Color::Yellow);
        assert_eq!(m.color(Direction::BottomRight), Color::Yellow);
        assert_eq!(m.color(Direction::BottomLeft), Color::Blue);
        assert_eq!(m.color(Direction::Left), Color::Red);
        assert_eq!(m.color(Direction::TopLeft), Color::Red);

        let m = Move {
            tile: Tile::RYYBBR,
            position: Position::CENTER,
            rotation: Rotation::R300,
            forced: false,
        };
        assert_eq!(m.color(Direction::BottomLeft), Color::Red);
        assert_eq!(m.color(Direction::Left), Color::Yellow);
        assert_eq!(m.color(Direction::TopLeft), Color::Yellow);
        assert_eq!(m.color(Direction::TopRight), Color::Blue);
        assert_eq!(m.color(Direction::Right), Color::Blue);
        assert_eq!(m.color(Direction::BottomRight), Color::Red);
    }
}
