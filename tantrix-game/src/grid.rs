//! A dense map over a board's positions, for lookups without searching the
//! board's sorted moves: built once for a pass over the board (its legal
//! moves, its scores), then read many times.

use crate::{Board, Position};

/// What a position holds, as far as a pass over the board is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cell {
    Empty,
    /// The tile at this board index.
    Tile(u8),
    /// The empty position at this index among a pass's spaces.
    Space(u16),
}

/// The board's bounding box and one more position all round, so every
/// empty position next to a tile is inside it. Cells are laid out by `x`,
/// then `y`: the order of [`Position`]s.
pub(crate) struct Grid {
    min_x: i16,
    min_y: i16,
    width: i16,
    height: i16,
    cells: Vec<Cell>,
}

impl Grid {
    /// The tiles of `board`, by position. `board` must not be empty.
    pub(crate) fn of(board: &Board) -> Self {
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (i16::MAX, i16::MIN, i16::MAX, i16::MIN);
        for m in board.iter() {
            let (x, y) = (m.position.x() as i16, m.position.y() as i16);
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        let (width, height) = (max_x - min_x + 3, max_y - min_y + 3);
        let mut grid = Self {
            min_x: min_x - 1,
            min_y: min_y - 1,
            width,
            height,
            cells: vec![Cell::Empty; width as usize * height as usize],
        };
        for (index, m) in board.iter().enumerate() {
            grid.set(m.position, Cell::Tile(index as u8));
        }
        grid
    }

    fn index(&self, pos: Position) -> Option<usize> {
        let x = pos.x() as i16 - self.min_x;
        let y = pos.y() as i16 - self.min_y;
        ((0..self.width).contains(&x) && (0..self.height).contains(&y))
            .then(|| x as usize * self.height as usize + y as usize)
    }

    /// What `pos` holds: [`Cell::Empty`] outside the grid.
    pub(crate) fn get(&self, pos: Position) -> Cell {
        self.index(pos).map_or(Cell::Empty, |i| self.cells[i])
    }

    /// The board index of the tile at `pos`, if there is one.
    pub(crate) fn tile(&self, pos: Position) -> Option<usize> {
        match self.get(pos) {
            Cell::Tile(index) => Some(index as usize),
            _ => None,
        }
    }

    /// The space index at `pos`, if it is one.
    pub(crate) fn space(&self, pos: Position) -> Option<usize> {
        match self.get(pos) {
            Cell::Space(index) => Some(index as usize),
            _ => None,
        }
    }

    /// Set `pos`, which must be inside the grid.
    pub(crate) fn set(&mut self, pos: Position, cell: Cell) {
        let index = self.index(pos).expect("set inside the grid");
        self.cells[index] = cell;
    }

    fn position(&self, index: usize) -> Position {
        let (x, y) = (index / self.height as usize, index % self.height as usize);
        Position::new((x as i16 + self.min_x) as i8, (y as i16 + self.min_y) as i8)
    }

    /// Number the cells holding `cell` as spaces, in [`Position`] order:
    /// they become `Cell::Space(0)`, `Cell::Space(1)`…
    pub(crate) fn number_spaces(&mut self, cell: Cell) {
        for (number, c) in self.cells.iter_mut().filter(|c| **c == cell).enumerate() {
            *c = Cell::Space(number as u16);
        }
    }

    /// The positions of the spaces, in [`Position`] order: by their number
    /// once [`Self::number_spaces`] has numbered them.
    pub(crate) fn spaces(&self) -> impl Iterator<Item = Position> + '_ {
        self.cells
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c, Cell::Space(_)))
            .map(|(i, _)| self.position(i))
    }
}
