//! A set of tiles, one bit per tile.

use std::sync::LazyLock;

use tantrix_game::{Board, Edges, Hand, Tile};

/// Every tile, by its index (`tile as usize`).
static TILES: LazyLock<Vec<Tile>> = LazyLock::new(|| enum_iterator::all::<Tile>().collect());

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TileSet(u64);

impl TileSet {
    /// The tiles not on `board`: the ones still to be placed.
    pub(crate) fn off(board: &Board) -> Self {
        let on_board = board.iter().fold(0, |bits, m| bits | 1 << m.tile as u64);
        Self(!on_board & ((1 << TILES.len()) - 1))
    }

    pub(crate) fn of(hand: &Hand) -> Self {
        Self(hand.iter().fold(0, |bits, &tile| bits | 1 << tile as u64))
    }

    pub(crate) fn without(self, tile: Tile) -> Self {
        Self(self.0 & !(1 << tile as u64))
    }

    pub(crate) fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    pub(crate) fn iter(self) -> impl Iterator<Item = Tile> {
        let mut bits = self.0;
        std::iter::from_fn(move || {
            (bits != 0).then(|| {
                let index = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                TILES[index]
            })
        })
    }

    /// Whether some tile of the set fits a space with these `edges`.
    pub(crate) fn any_fits(self, edges: Edges) -> bool {
        self.iter().any(|tile| edges.accepts(tile))
    }
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Board, Move, Position, Rotation, Tile};

    use super::TileSet;

    #[test]
    fn the_tiles_off_an_empty_board_are_all_of_them() {
        let all = TileSet::off(&Board::default());
        assert_eq!(all.len(), 56);
        assert!(all.iter().eq(enum_iterator::all::<Tile>()));

        let mut board = Board::default();
        board
            .insert_move(Move {
                tile: Tile::GBBYGY,
                position: Position::CENTER,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();
        let off = TileSet::off(&board);
        assert_eq!(off.len(), 55);
        assert!(!off.iter().any(|t| t == Tile::GBBYGY));
        assert_eq!(off.without(Tile::RYYBRB).len(), 54);
    }
}
