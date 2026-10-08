//! Legal move generation: the golden rule (touching edges match), forced
//! spaces, and the restriction rules that apply while the bag has tiles.

use std::collections::BTreeSet;

use arrayvec::ArrayVec;
use enum_map::{enum_map, EnumMap};

use crate::edges::LOW_BITS;
use crate::grid::{Cell, Grid};
use crate::{Board, Color, Direction, Edges, Hand, Move, Position, Rotation, Tile, TurnStep};

impl Board {
    pub fn position_is_forced(&self, pos: &Position) -> bool {
        self.neighbor_count(pos) >= 3
    }

    /// Rule 1: Would placing `m` create a forced space where all surrounding
    /// links are the same color (making it impossible to fill)?
    pub fn creates_same_color_triple(&self, m: &Move) -> bool {
        // Check each empty neighbor of the move's position
        for (neighbor_pos, _) in m.position.around() {
            if !self.is_available(&neighbor_pos) {
                continue; // occupied, not a space we're creating
            }
            // Count how many neighbors this empty space would have (existing + the new tile)
            let existing_neighbors = self.neighbor_count(&neighbor_pos);
            let total_neighbors = existing_neighbors + 1; // +1 for the tile being placed
            if total_neighbors < 3 {
                continue; // not a forced space
            }
            // Collect all colors of links pointing into this empty space
            let mut colors: ArrayVec<Color, 6> = ArrayVec::new();
            for (adj_pos, adj_dir) in neighbor_pos.around() {
                if adj_pos == m.position {
                    // The tile being placed
                    colors.push(m.color(adj_dir.opposite()));
                } else if let Some(adj_move) = self.get_move(&adj_pos) {
                    colors.push(adj_move.color(adj_dir.opposite()));
                }
            }
            // If all colors are identical, no tile could ever fit
            if colors.len() >= 3 && colors.windows(2).all(|w| w[0] == w[1]) {
                return true;
            }
        }
        false
    }

    /// Returns all contour positions that lie on a "controlled side" of a forced space.
    /// For each forced position, finds boundary directions (empty dirs adjacent to occupied
    /// dirs) and traces straight-line rays outward. A ray continues while the position is
    /// on the contour and is not itself forced.
    pub fn controlled_positions(
        &self,
        contour_positions: &BTreeSet<Position>,
    ) -> BTreeSet<Position> {
        let mut controlled = BTreeSet::new();

        for &pos in contour_positions {
            if !self.position_is_forced(&pos) {
                continue;
            }

            // Classify each of 6 directions as occupied or empty
            let mut occupied = [false; 6];
            for (neighbor_pos, dir) in pos.around() {
                if !self.is_available(&neighbor_pos) {
                    occupied[dir.index()] = true;
                }
            }

            // Find ray directions: empty directions adjacent to an occupied direction
            for d in 0..6 {
                if !occupied[d] && (occupied[(d + 5) % 6] || occupied[(d + 1) % 6]) {
                    let dir = Direction::from(d as i8);
                    let mut p = pos.neighbor(dir);
                    while contour_positions.contains(&p) && !self.position_is_forced(&p) {
                        controlled.insert(p);
                        p = p.neighbor(dir);
                    }
                }
            }
        }

        controlled
    }

    /// Compute all valid moves for tiles in the given hand at `step` of the turn.
    /// Forced spaces must be filled first whenever a tile fits, including
    /// after the bag empties. Restriction rules (controlled sides and
    /// same-color triples) apply only while the bag has tiles.
    ///
    /// Moves come tile by tile in hand order, then by position, then by
    /// rotation. The placements are the same as checking every tile, contour
    /// position and rotation with [`Self::get_possible_move`],
    /// [`Self::controlled_positions`] and [`Self::creates_same_color_triple`]
    /// (the tests compare them), but what those look up about a position is
    /// worked out once per position, and forced spaces are tried first: other
    /// moves are only generated when no forced move is possible.
    pub fn possible_moves(&self, hand: &Hand, bag_is_empty: bool, step: TurnStep) -> Vec<Move> {
        let restricted = !bag_is_empty;
        let spaces = self.spaces(restricted);

        let forced = moves_into(hand, &spaces, |space| space.forced);
        if !forced.is_empty() || step == TurnStep::AfterFreeMove {
            return forced;
        }
        // Forced spaces have no move left to offer, and controlled sides
        // take no tile while the restriction rules apply.
        moves_into(hand, &spaces, |space| !space.forced && !space.controlled)
    }

    /// The empty positions on the contour, sorted like
    /// [`Self::contour_positions`], with what the rules need to know about
    /// each. `restricted`: the restriction rules apply, so controlled sides and
    /// same-color triples are worked out too.
    fn spaces(&self, restricted: bool) -> Vec<Space> {
        if self.is_empty() {
            return vec![Space::new(Position::CENTER, Edges::default())];
        }
        // Mark the empty positions next to a tile, then number them in
        // position order.
        let mut grid = Grid::of(self);
        const UNNUMBERED: Cell = Cell::Space(u16::MAX);
        for m in self.iter() {
            for (pos, _) in m.position.around() {
                if grid.get(pos) == Cell::Empty {
                    grid.set(pos, UNNUMBERED);
                }
            }
        }
        grid.number_spaces(UNNUMBERED);
        let mut spaces: Vec<Space> = grid
            .spaces()
            .map(|position| {
                let mut edges = Edges::default();
                for (neighbor, dir) in position.around() {
                    if let Some(index) = grid.tile(neighbor) {
                        let color = self[index].edges().get(dir.opposite());
                        edges.set(dir, color.expect("a tile's edges are all set"));
                    }
                }
                Space::new(position, edges)
            })
            .collect();

        if restricted {
            let uniform: Vec<Option<Color>> = spaces.iter().map(Space::uniform_color).collect();
            let triples: Vec<_> = spaces.iter().map(|s| s.triples(&uniform, &grid)).collect();
            let controlled = controlled_spaces(&spaces, &grid);
            for ((space, triples), controlled) in spaces.iter_mut().zip(triples).zip(controlled) {
                space.triples = triples;
                space.controlled = controlled;
            }
        }
        spaces
    }

    /// The colors the tiles around `pos` show it, by direction: what a tile
    /// placed there must match ([`Edges::matches`]).
    pub fn edges_toward(&self, pos: Position) -> Edges {
        let mut edges = Edges::default();
        for (neighbor, dir) in pos.around() {
            if let Some(m) = self.get_move(&neighbor) {
                edges.set(dir, m.color(dir.opposite()));
            }
        }
        edges
    }

    pub fn get_possible_move(
        &self,
        tile: Tile,
        position: Position,
    ) -> impl Iterator<Item = Move> + '_ {
        let mut rotation_possible: EnumMap<Rotation, bool> = enum_map! {_ => true};

        let around_colors = position
            .around()
            .filter_map(|(pos, dir)| self.get_move(&pos).map(|m| (dir, m.color(dir.opposite()))));

        for (color_position, color) in around_colors {
            for piece_rotation in enum_iterator::all::<Rotation>() {
                let possible = rotation_possible[piece_rotation];
                let same_color = color == tile.color(color_position - piece_rotation);
                if possible && !same_color {
                    rotation_possible[piece_rotation] = false
                }
            }
        }

        rotation_possible
            .into_iter()
            .filter_map(|(rot, possible)| possible.then_some(rot))
            .map(move |rotation| Move {
                tile,
                rotation,
                position,
                forced: self.position_is_forced(&position),
            })
    }
}

/// The golden rule, on packed edges.
impl Edges {
    /// Whether `tile` (a tile's edges, all set) shows the same color on
    /// every edge set here: the golden rule, for a space's edges.
    pub fn matches(self, tile: Edges) -> bool {
        (self.colors ^ tile.colors) & self.known == 0
    }

    /// Whether `tile` (a tile's edges, all set) shows another color on every
    /// edge set here.
    pub fn avoids(self, tile: Edges) -> bool {
        let differ = self.colors ^ tile.colors;
        // The low bit of each direction whose two bits differ somewhere.
        let differs = (differ | differ >> 1) & LOW_BITS;
        differs & self.known == self.known & LOW_BITS
    }

    /// The rotations of `tile` that [`Self::matches`] these edges.
    pub fn rotations(self, tile: Tile) -> impl Iterator<Item = Rotation> {
        let turned = tile.all_edges();
        enum_iterator::all::<Rotation>()
            .filter(move |&rotation| self.matches(turned[rotation as usize]))
    }

    /// Whether some rotation of `tile` [`Self::matches`] these edges.
    pub fn accepts(self, tile: Tile) -> bool {
        tile.all_edges().iter().any(|&turned| self.matches(turned))
    }
}

/// One empty position on the contour, as [`Board::possible_moves`] sees it.
struct Space {
    position: Position,
    /// The color each neighboring tile shows this space, by direction.
    edges: Edges,
    /// Three or more neighbors while the bag has tiles: must be filled first.
    forced: bool,
    /// On a controlled side of a forced space ([`Board::controlled_positions`]).
    /// Only worked out while the restriction rules apply.
    controlled: bool,
    /// By direction, the color a tile placed here must not show that way: the
    /// neighboring space has two or more neighbors already, all showing it
    /// that color, so a third would make a same-color triple
    /// ([`Board::creates_same_color_triple`]). Only worked out while the
    /// restriction rules apply.
    triples: Edges,
}

impl Space {
    fn new(position: Position, edges: Edges) -> Self {
        Self {
            position,
            edges,
            forced: edges.count() >= 3,
            controlled: false,
            triples: Edges::default(),
        }
    }

    /// The color every neighbor shows this space, when it has two or more.
    fn uniform_color(&self) -> Option<Color> {
        let mut colors = enum_iterator::all::<Direction>().filter_map(|dir| self.edges.get(dir));
        let first = colors.next()?;
        let mut count = 1;
        for color in colors {
            if color != first {
                return None;
            }
            count += 1;
        }
        (count >= 2).then_some(first)
    }

    /// [`Self::triples`], given the [`Self::uniform_color`] of every space
    /// on the contour, numbered in `grid`. An empty neighbor with a
    /// neighbor of its own is on the contour too.
    fn triples(&self, uniform: &[Option<Color>], grid: &Grid) -> Edges {
        let mut triples = Edges::default();
        for (neighbor, dir) in self.position.around() {
            if let Some(color) = grid.space(neighbor).and_then(|i| uniform[i]) {
                triples.set(dir, color);
            }
        }
        triples
    }

    /// Whether a tile showing `edges` may be placed here: every edge
    /// matches its neighbor's color (the golden rule) and completes no
    /// same-color triple.
    fn fits(&self, edges: Edges) -> bool {
        self.edges.matches(edges) && self.triples.avoids(edges)
    }
}

/// Which `spaces` (numbered in `grid`) lie on a controlled side of a forced
/// space, by the rays of [`Board::controlled_positions`].
fn controlled_spaces(spaces: &[Space], grid: &Grid) -> Vec<bool> {
    let mut controlled = vec![false; spaces.len()];
    for space in spaces.iter().filter(|space| space.forced) {
        let occupied: [bool; 6] =
            std::array::from_fn(|d| space.edges.get(Direction::from(d as i8)).is_some());
        for d in 0..6 {
            if !occupied[d] && (occupied[(d + 5) % 6] || occupied[(d + 1) % 6]) {
                let dir = Direction::from(d as i8);
                let mut p = space.position.neighbor(dir);
                while let Some(i) = grid.space(p).filter(|&i| !spaces[i].forced) {
                    controlled[i] = true;
                    p = p.neighbor(dir);
                }
            }
        }
    }
    controlled
}

/// Every move of `hand` into the `spaces` that `include` keeps, tile by tile
/// in hand order, then by position, then by rotation.
fn moves_into(hand: &Hand, spaces: &[Space], include: impl Fn(&Space) -> bool) -> Vec<Move> {
    let mut moves = Vec::new();
    for &tile in hand.iter() {
        let turned = tile.all_edges();
        for space in spaces.iter().filter(|space| include(space)) {
            for rotation in enum_iterator::all::<Rotation>() {
                if space.fits(turned[rotation as usize]) {
                    moves.push(Move {
                        tile,
                        position: space.position,
                        rotation,
                        forced: space.forced,
                    });
                }
            }
        }
    }
    moves
}

#[cfg(test)]
mod tests {
    use crate::test_games::for_each_seeded_state;
    use crate::{Board, Color, Direction, Edges, Hand, Move, Position, Rotation, Tile, TurnStep};

    /// [`Board::possible_moves`] move by move, straight from the rules:
    /// every tile, contour position and rotation, then the restriction
    /// rules, then the forced-move priority.
    fn possible_moves_one_by_one(
        board: &Board,
        hand: &Hand,
        bag_is_empty: bool,
        step: TurnStep,
    ) -> Vec<Move> {
        let mut contour_positions = board.contour_positions();
        if !bag_is_empty {
            let controlled = board.controlled_positions(&contour_positions);
            contour_positions = contour_positions.difference(&controlled).cloned().collect();
        }
        let mut moves: Vec<Move> = hand
            .iter()
            .copied()
            .flat_map(|tile| {
                contour_positions
                    .iter()
                    .flat_map(move |pos| board.get_possible_move(tile, *pos))
            })
            .collect();
        if !bag_is_empty {
            moves.retain(|m| !board.creates_same_color_triple(m));
        }
        if step == TurnStep::AfterFreeMove || moves.iter().any(|m| m.forced) {
            moves.retain(|m| m.forced);
        }
        moves
    }

    #[test]
    fn edges_match_and_avoid_only_the_edges_set() {
        let tile = Tile::BYYBRR.edges(Rotation::None); // B Y Y B R R
        let mut space = Edges::default();
        assert!(space.matches(tile) && space.avoids(tile));
        assert_eq!(space.get(Direction::Left), None);

        space.set(Direction::Left, Color::Blue);
        assert!(space.matches(tile) && !space.avoids(tile));
        space.set(Direction::Right, Color::Red);
        assert!(!space.matches(tile) && !space.avoids(tile));
        space.set(Direction::Left, Color::Green);
        assert!(!space.matches(tile) && space.avoids(tile));
        assert_eq!(space.get(Direction::Left), Some(Color::Green));
        assert_eq!(space.count(), 2);

        // Some rotation of the tile fits Blue on the left and Red on the right.
        let mut space = Edges::default();
        space.set(Direction::Left, Color::Blue);
        space.set(Direction::BottomRight, Color::Red);
        assert!(space.accepts(Tile::BYYBRR));
        assert!(space
            .rotations(Tile::BYYBRR)
            .all(|r| space.matches(Tile::BYYBRR.edges(r))));
        assert_eq!(space.rotations(Tile::BYYBRR).count(), 1);
    }

    #[test]
    fn possible_moves_match_the_rules_move_by_move() {
        let mut checked = 0;
        let mut forced = 0;
        for_each_seeded_state(12, |game| {
            // Every hand on every board, with and without the restriction
            // rules, at both steps of a turn.
            for &color in game.playing_colors() {
                let hand = game.hand(color);
                for bag_is_empty in [false, true] {
                    for step in [TurnStep::BeforeFreeMove, TurnStep::AfterFreeMove] {
                        let moves = game.board().possible_moves(hand, bag_is_empty, step);
                        assert_eq!(
                            moves,
                            possible_moves_one_by_one(game.board(), hand, bag_is_empty, step),
                            "{color:?}, bag empty: {bag_is_empty}, {step:?}, board {:?}",
                            game.board()
                        );
                        checked += 1;
                        forced += usize::from(moves.first().is_some_and(|m| m.forced));
                    }
                }
            }
        });
        // The games reach forced spaces, not only free moves.
        assert!(
            checked > 5_000 && forced > 500,
            "{checked} checked, {forced} forced"
        );
        println!("{checked} checked, {forced} forced");
    }

    #[test]
    fn rule_no_same_color_triple() {
        // Place 2 tiles around CENTER with Blue edges pointing into CENTER.
        // A 3rd tile completing the triple should be rejected.
        //
        // BYYBRR at Left(-1,0), Rotation::None: Right edge = B
        // RYYRBB at TopLeft(0,1), Rotation::None: BottomRight edge = B
        let mut board = Board::default();
        let left = Position::CENTER.neighbor(Direction::Left);
        let top_left = Position::CENTER.neighbor(Direction::TopLeft);
        let top_right = Position::CENTER.neighbor(Direction::TopRight);

        board
            .insert_move(Move {
                tile: Tile::BYYBRR,
                position: left,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();
        board
            .insert_move(Move {
                tile: Tile::RYYRBB,
                position: top_left,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();

        // RYYBRB at TopRight(1,1), Rotation::None: BottomLeft edge = B
        // This creates a forced space at CENTER with all-Blue edges → illegal
        let bad_move = Move {
            tile: Tile::RYYBRB,
            position: top_right,
            rotation: Rotation::None,
            forced: false,
        };
        assert!(
            board.creates_same_color_triple(&bad_move),
            "should detect same-color triple (all Blue into CENTER)"
        );

        // BYRBRY at TopRight(1,1), Rotation::None: BottomLeft edge = Y
        // This creates a forced space at CENTER with B, B, Y → not all same
        let ok_move = Move {
            tile: Tile::BYRBRY,
            position: top_right,
            rotation: Rotation::None,
            forced: false,
        };
        assert!(
            !board.creates_same_color_triple(&ok_move),
            "should allow mixed colors into forced space"
        );
    }

    #[test]
    fn rule_no_four_sided_space() {
        // Place 3 tiles around CENTER, then a 4th that gives CENTER 4 neighbors.
        let mut board = Board::default();
        let left = Position::CENTER.neighbor(Direction::Left);
        let top_left = Position::CENTER.neighbor(Direction::TopLeft);
        let top_right = Position::CENTER.neighbor(Direction::TopRight);
        let right = Position::CENTER.neighbor(Direction::Right);

        board
            .insert_move(Move {
                tile: Tile::BYYBRR,
                position: left,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();
        board
            .insert_move(Move {
                tile: Tile::RYYRBB,
                position: top_left,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();
        board
            .insert_move(Move {
                tile: Tile::RYYBRB,
                position: top_right,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();

        let contour = board.contour_positions();
        // Placing at Right would give CENTER a 4th neighbor → illegal (controlled side)
        let controlled = board.controlled_positions(&contour);
        assert!(
            controlled.contains(&right),
            "right should be on a controlled side (CENTER is forced with 3 neighbors)"
        );

        // Placing far away from CENTER should be fine
        let far = left.neighbor(Direction::Left).neighbor(Direction::Left);
        assert!(
            !controlled.contains(&far),
            "far position should not be on a controlled side"
        );
    }

    #[test]
    fn rule_no_controlled_sides() {
        // Place 3 tiles around CENTER to make it a forced space.
        // Then trying to place at another neighbor of CENTER should be rejected
        // (it's a controlled side), but filling CENTER itself should be allowed.
        let mut board = Board::default();
        let left = Position::CENTER.neighbor(Direction::Left);
        let top_left = Position::CENTER.neighbor(Direction::TopLeft);
        let top_right = Position::CENTER.neighbor(Direction::TopRight);
        let right = Position::CENTER.neighbor(Direction::Right);

        board
            .insert_move(Move {
                tile: Tile::BYYBRR,
                position: left,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();
        board
            .insert_move(Move {
                tile: Tile::RYYRBB,
                position: top_left,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();
        board
            .insert_move(Move {
                tile: Tile::RYYBRB,
                position: top_right,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();

        let contour = board.contour_positions();
        // CENTER is now a forced space (3 neighbors).
        // Right is adjacent to CENTER and on the contour → controlled side → should be blocked
        let controlled = board.controlled_positions(&contour);
        assert!(
            controlled.contains(&right),
            "should reject move along controlled side of forced space"
        );

        // CENTER itself (the forced space) should NOT be in the controlled set
        assert!(
            !controlled.contains(&Position::CENTER),
            "filling the forced space itself should be allowed"
        );
    }
}
