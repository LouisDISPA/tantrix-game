//! A tile placed (or to be placed) on the board.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayedMove {
    pub m: Move,
    pub hand_index: usize,
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
