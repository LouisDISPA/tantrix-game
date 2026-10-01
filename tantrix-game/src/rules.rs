//! Legal move generation: the golden rule (touching edges match), forced
//! spaces, and the restriction rules that apply while the bag has tiles.

use std::collections::BTreeSet;

use arrayvec::ArrayVec;
use enum_map::{enum_map, EnumMap};

use crate::{Board, Color, Direction, Hand, Move, Position, Rotation, Tile, TurnStep};

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
    /// While the bag has tiles, restriction rules (controlled sides,
    /// same-color triples) apply and forced spaces must be filled first.
    /// Once the bag is empty, every fitting placement is a free move.
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

        if bag_is_empty {
            return if step == TurnStep::AfterFreeMove {
                Vec::new()
            } else {
                moves_into(hand, &spaces, |_| true)
            };
        }

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
        let mut positions: Vec<Position> = if self.is_empty() {
            vec![Position::CENTER]
        } else {
            self.iter()
                .flat_map(|m| m.position.around())
                .map(|(pos, _)| pos)
                .filter(|pos| self.is_available(pos))
                .collect()
        };
        positions.sort_unstable();
        positions.dedup();

        let mut spaces: Vec<Space> = positions
            .into_iter()
            .map(|position| {
                let mut edges = [None; 6];
                for (neighbor, dir) in position.around() {
                    if let Some(m) = self.get_move(&neighbor) {
                        edges[dir.index()] = Some(m.color(dir.opposite()));
                    }
                }
                let neighbors = edges.iter().flatten().count();
                Space {
                    position,
                    edges,
                    forced: restricted && neighbors >= 3,
                    controlled: false,
                    triples: [None; 6],
                }
            })
            .collect();

        if restricted {
            let triples: Vec<_> = spaces.iter().map(|s| s.triples(&spaces)).collect();
            let controlled = controlled_spaces(&spaces);
            for ((space, triples), controlled) in spaces.iter_mut().zip(triples).zip(controlled) {
                space.triples = triples;
                space.controlled = controlled;
            }
        }
        spaces
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

/// One empty position on the contour, as [`Board::possible_moves`] sees it.
struct Space {
    position: Position,
    /// The color each neighboring tile shows this space, by direction.
    edges: [Option<Color>; 6],
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
    triples: [Option<Color>; 6],
}

impl Space {
    /// The color every neighbor shows this space, when it has two or more.
    fn uniform_color(&self) -> Option<Color> {
        let mut colors = self.edges.iter().flatten();
        let first = *colors.next()?;
        let mut count = 1;
        for &color in colors {
            if color != first {
                return None;
            }
            count += 1;
        }
        (count >= 2).then_some(first)
    }

    /// [`Self::triples`], given every space on the contour. An empty
    /// neighbor with a neighbor of its own is on the contour too.
    fn triples(&self, spaces: &[Space]) -> [Option<Color>; 6] {
        let mut triples = [None; 6];
        for (neighbor, dir) in self.position.around() {
            if let Some(space) = find_space(spaces, neighbor) {
                triples[dir.index()] = spaces[space].uniform_color();
            }
        }
        triples
    }

    /// Whether `tile` turned by `rotation` may be placed here: every edge
    /// matches its neighbor's color (the golden rule) and completes no
    /// same-color triple.
    fn fits(&self, tile: Tile, rotation: Rotation) -> bool {
        enum_iterator::all::<Direction>().all(|dir| {
            let color = tile.color(dir - rotation);
            let d = dir.index();
            self.edges[d].is_none_or(|edge| edge == color) && self.triples[d] != Some(color)
        })
    }
}

fn find_space(spaces: &[Space], position: Position) -> Option<usize> {
    spaces
        .binary_search_by(|space| space.position.cmp(&position))
        .ok()
}

/// Which `spaces` lie on a controlled side of a forced space, by the rays of
/// [`Board::controlled_positions`].
fn controlled_spaces(spaces: &[Space]) -> Vec<bool> {
    let mut controlled = vec![false; spaces.len()];
    for space in spaces.iter().filter(|space| space.forced) {
        let occupied = space.edges.map(|edge| edge.is_some());
        for d in 0..6 {
            if !occupied[d] && (occupied[(d + 5) % 6] || occupied[(d + 1) % 6]) {
                let dir = Direction::from(d as i8);
                let mut p = space.position.neighbor(dir);
                while let Some(i) = find_space(spaces, p).filter(|&i| !spaces[i].forced) {
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
        for space in spaces.iter().filter(|space| include(space)) {
            for rotation in enum_iterator::all::<Rotation>() {
                if space.fits(tile, rotation) {
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
    use crate::{Board, Direction, Hand, Move, Position, Rotation, Tile, TurnStep};

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
        if bag_is_empty {
            if step == TurnStep::AfterFreeMove {
                return Vec::new();
            }
            for m in &mut moves {
                m.forced = false;
            }
        } else if step == TurnStep::AfterFreeMove || moves.iter().any(|m| m.forced) {
            moves.retain(|m| m.forced);
        }
        moves
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
