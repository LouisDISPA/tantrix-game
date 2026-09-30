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
    /// `bag_is_empty` controls whether restriction rules (controlled sides, same-color triples) apply.
    /// Forced spaces must be filled first, and once the free move is played only
    /// forced spaces remain.
    pub fn possible_moves(&self, hand: &Hand, bag_is_empty: bool, step: TurnStep) -> Vec<Move> {
        let mut contour_positions = self.contour_positions();

        if !bag_is_empty {
            let controlled = self.controlled_positions(&contour_positions);
            contour_positions = contour_positions.difference(&controlled).cloned().collect();
        }

        let mut moves: Vec<Move> = hand
            .iter()
            .copied()
            .flat_map(|tile| {
                contour_positions
                    .iter()
                    .flat_map(move |pos| self.get_possible_move(tile, *pos))
            })
            .collect();

        if !bag_is_empty {
            moves.retain(|m| !self.creates_same_color_triple(m));
        }

        if step == TurnStep::AfterFreeMove || moves.iter().any(|m| m.forced) {
            moves.retain(|m| m.forced);
        }

        moves
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

#[cfg(test)]
mod tests {
    use crate::{Board, Direction, Move, Position, Rotation, Tile};

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
