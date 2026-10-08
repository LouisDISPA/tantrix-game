//! The colors on a hex's six edges, packed two bits per direction so that a
//! tile can be checked against every edge of a space at once.

use std::sync::LazyLock;

use crate::tiles::NUMBER_OF_TILE;
use crate::{Color, Direction, Rotation, Tile};

/// The low bit of each direction's two.
pub(crate) const LOW_BITS: u16 = 0b0101_0101_0101;

/// Colors on some of a hex's six edges, by direction: a placed tile's (all
/// six, [`Tile::edges`]), or the colors the tiles around an empty position
/// show it ([`Board::edges_toward`](crate::Board::edges_toward)). Whether a
/// tile fits them is the rules' to say (`rules.rs`: [`Edges::matches`]…).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Edges {
    /// Two bits per direction, in [`Direction::index`] order: the color.
    pub(crate) colors: u16,
    /// `0b11` for each direction whose color is set.
    pub(crate) known: u16,
}

impl Edges {
    fn shift(dir: Direction) -> u16 {
        dir.index() as u16 * 2
    }

    /// The color on edge `dir`, if it is set.
    pub fn get(self, dir: Direction) -> Option<Color> {
        let shift = Self::shift(dir);
        (self.known >> shift & 0b11 != 0).then(|| color_of(self.colors >> shift & 0b11))
    }

    /// Set edge `dir` to `color`.
    pub fn set(&mut self, dir: Direction, color: Color) {
        let shift = Self::shift(dir);
        self.colors = self.colors & !(0b11 << shift) | (color as u16) << shift;
        self.known |= 0b11 << shift;
    }

    /// How many edges are set.
    pub fn count(self) -> usize {
        (self.known & LOW_BITS).count_ones() as usize
    }

    /// The other edge with the color of edge `dir`, for a tile's edges (each
    /// of its colors is on exactly two edges).
    pub fn other_end(self, dir: Direction) -> Direction {
        let shift = Self::shift(dir);
        // Zero in every direction showing the color of `dir`…
        let differ = self.colors ^ ((self.colors >> shift & 0b11) * LOW_BITS);
        // …so these are the low bits of the directions that show it, but `dir`.
        let same = !(differ | differ >> 1) & LOW_BITS & !(1 << shift);
        debug_assert_eq!(same.count_ones(), 1, "a tile shows each color on two edges");
        Direction::from((same.trailing_zeros() / 2) as i8)
    }
}

fn color_of(bits: u16) -> Color {
    match bits {
        0 => Color::Red,
        1 => Color::Blue,
        2 => Color::Yellow,
        _ => Color::Green,
    }
}

/// Every tile's edges, by rotation.
static TILE_EDGES: LazyLock<[[Edges; 6]; NUMBER_OF_TILE]> = LazyLock::new(|| {
    let mut table = [[Edges::default(); 6]; NUMBER_OF_TILE];
    for tile in enum_iterator::all::<Tile>() {
        for rotation in enum_iterator::all::<Rotation>() {
            let edges = &mut table[tile as usize][rotation as usize];
            for dir in enum_iterator::all::<Direction>() {
                edges.set(dir, tile.color(dir - rotation));
            }
        }
    }
    table
});

impl Tile {
    /// The colors of this tile's edges, turned by `rotation`.
    pub fn edges(self, rotation: Rotation) -> Edges {
        TILE_EDGES[self as usize][rotation as usize]
    }

    /// [`Self::edges`] for every rotation, in [`Rotation`] order.
    pub(crate) fn all_edges(self) -> &'static [Edges; 6] {
        &TILE_EDGES[self as usize]
    }
}

#[cfg(test)]
mod tests {
    use crate::{Direction, Move, Position, Rotation, Tile};

    #[test]
    fn tile_edges_follow_the_rotation() {
        for tile in enum_iterator::all::<Tile>() {
            for rotation in enum_iterator::all::<Rotation>() {
                let m = Move {
                    tile,
                    position: Position::CENTER,
                    rotation,
                    forced: false,
                };
                let edges = tile.edges(rotation);
                assert_eq!(edges.count(), 6);
                for dir in enum_iterator::all::<Direction>() {
                    assert_eq!(edges.get(dir), Some(m.color(dir)));
                    let other = edges.other_end(dir);
                    assert_ne!(other, dir);
                    assert_eq!(m.color(other), m.color(dir));
                }
            }
        }
    }
}
