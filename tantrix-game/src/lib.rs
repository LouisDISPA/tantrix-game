use std::collections::BTreeSet;

use arrayvec::ArrayVec;
use enum_map::{enum_map, EnumMap};
use rand::prelude::SliceRandom;

mod coordinate_system;
pub use coordinate_system::*;

mod tiles;
pub use tiles::*;

#[derive(Debug, Clone, Copy)]
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

#[derive(Default, Debug, Clone)]
pub struct Board {
    moves: ArrayVec<Move, NUMBER_OF_TILE>,
}

impl Board {
    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    fn is_available(&self, pos: &Position) -> bool {
        self.moves
            .binary_search_by(|m| m.position.cmp(pos))
            .is_err()
    }

    fn insert_move(&mut self, m: Move) -> Result<(), String> {
        let index = match self.moves.binary_search_by(|i| i.position.cmp(&m.position)) {
            Ok(_) => return Err("Position unavailable".to_string()),
            Err(i) => i,
        };
        self.moves.try_insert(index, m).map_err(|e| e.to_string())
    }

    fn neighbor_count(&self, pos: &Position) -> u8 {
        pos.around().filter(|(p, _)| !self.is_available(p)).count() as u8
    }

    fn possition_is_forced(&self, pos: &Position) -> bool {
        self.neighbor_count(pos) >= 3
    }

    /// Rule 1: Would placing `m` create a forced space where all surrounding
    /// links are the same color (making it impossible to fill)?
    fn creates_same_color_triple(&self, m: &Move) -> bool {
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
                    colors.push(m.color(adj_dir.oposite()));
                } else if let Some(adj_move) = self.get_move(&adj_pos) {
                    colors.push(adj_move.color(adj_dir.oposite()));
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
    fn controlled_positions(&self, contour_positions: &BTreeSet<Position>) -> BTreeSet<Position> {
        let mut controlled = BTreeSet::new();

        for &pos in contour_positions {
            if !self.possition_is_forced(&pos) {
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
                    while contour_positions.contains(&p) && !self.possition_is_forced(&p) {
                        controlled.insert(p);
                        p = p.neighbor(dir);
                    }
                }
            }
        }

        controlled
    }

    fn get_move(&self, pos: &Position) -> Option<Move> {
        match self.moves.binary_search_by(|m| m.position.cmp(pos)) {
            Ok(index) => Some(self.moves[index]),
            Err(_) => None,
        }
    }

    fn countour_positions(&self) -> BTreeSet<Position> {
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

    fn get_possible_move(&self, tile: Tile, position: Position) -> impl Iterator<Item = Move> + '_ {
        let mut rotation_possible: EnumMap<Rotation, bool> = enum_map! {_ => true};

        let around_colors = position
            .around()
            .filter_map(|(pos, dir)| self.get_move(&pos).map(|m| (dir, m.color(dir.oposite()))));

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
                forced: self.possition_is_forced(&position),
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    Someone,
    Ai,
    None,
}

const TILES_IN_HAND: usize = 6;
const NUMBER_OF_COLOR: usize = enum_iterator::cardinality::<Color>();
const NUMBER_OF_TILE: usize = enum_iterator::cardinality::<Tile>();

#[derive(Debug)]
pub struct Game {
    board: Board,
    bag_of_tile: ArrayVec<Tile, NUMBER_OF_TILE>,
    players: EnumMap<Color, Player>,
    currently_playing: usize,
    color_cycle: ArrayVec<Color, NUMBER_OF_COLOR>,
    hands: EnumMap<Color, ArrayVec<Tile, TILES_IN_HAND>>,
    current_moves: Option<Vec<Move>>,
}

impl Game {
    pub fn new(red: Player, blue: Player, yellow: Player, green: Player) -> Self {
        let board = Default::default();

        let mut bag_of_tile: ArrayVec<Tile, NUMBER_OF_TILE> = Default::default();
        for i in enum_iterator::all::<Tile>() {
            bag_of_tile.push(i)
        }
        let mut rng = rand::rng();
        bag_of_tile.shuffle(&mut rng);

        let mut players = enum_map! {
            _ => Player::None
        };
        players[Color::Red] = red;
        players[Color::Blue] = blue;
        players[Color::Yellow] = yellow;
        players[Color::Green] = green;

        let mut color_cycle: ArrayVec<Color, NUMBER_OF_COLOR> = players
            .iter()
            .filter(|(_, p)| **p != Player::None)
            .map(|(c, _)| c)
            .collect();

        color_cycle.shuffle(&mut rng);

        let currently_playing = 0;

        let mut hands: EnumMap<Color, ArrayVec<Tile, TILES_IN_HAND>> = Default::default();

        for color in color_cycle.iter() {
            for _ in 0..TILES_IN_HAND {
                if let Some(tile) = bag_of_tile.pop() {
                    hands[*color].push(tile)
                }
            }
        }

        Self {
            board,
            bag_of_tile,
            players,
            currently_playing,
            color_cycle,
            hands,
            current_moves: None,
        }
    }

    pub fn currently_playing(&self) -> Color {
        self.color_cycle[self.currently_playing]
    }

    pub fn get_hand(&self, color: Color) -> &[Tile] {
        &self.hands[color]
    }

    pub fn get_possible_moves(&mut self) -> &[Move] {
        if self.current_moves.is_none() {
            let current_player = self.currently_playing();
            let board = self.board();
            let mut countour_positions = board.countour_positions();

            if !self.bag_of_tile.is_empty() {
                let controlled_positions = self.board.controlled_positions(&countour_positions);
                countour_positions = countour_positions
                    .difference(&controlled_positions)
                    .cloned()
                    .collect();
            }

            let mut moves: Vec<Move> = self
                .get_hand(current_player)
                .iter()
                .copied()
                .flat_map(|tile| {
                    countour_positions
                        .iter()
                        .flat_map(move |pos| board.get_possible_move(tile, *pos))
                })
                .collect();

            if !self.bag_of_tile.is_empty() {
                moves.retain(|m| !self.board.creates_same_color_triple(m));
            }

            if moves.iter().any(|m| m.forced) {
                moves.retain(|m| m.forced);
            }

            self.current_moves = Some(moves);
        }
        match &self.current_moves {
            Some(moves) => moves,
            None => unreachable!(),
        }
    }

    pub fn play_move(&mut self, m: impl Into<Move>) -> Result<(), String> {
        let current_player = self.currently_playing();
        let m: Move = m.into();

        if !self.get_hand(current_player).contains(&m.tile) {
            return Err("Tile not in hand".to_string());
        }

        let has_forced_move = self.get_possible_moves().iter().any(|pm| pm.forced);
        let move_is_forced = self.board().possition_is_forced(&m.position);
        if has_forced_move && !move_is_forced {
            return Err("Must play a forced move".to_string());
        }

        let tile = m.tile;
        self.board.insert_move(m)?;
        self.remove_tile(tile);
        self.draw_tiles();
        if !move_is_forced {
            self.next_player();
        }
        self.current_moves = None;
        Ok(())
    }

    fn remove_tile(&mut self, tile: Tile) {
        let color = self.currently_playing();
        let hand = &mut self.hands[color];
        let index = hand
            .iter()
            .position(|t| t == &tile)
            .expect("Can't remove tile not in hand");
        hand.remove(index);
    }

    fn draw_tiles(&mut self) {
        if !self.bag_of_tile.is_empty() {
            let color = self.currently_playing();
            let hand = &mut self.hands[color];
            let number_to_draw = TILES_IN_HAND - hand.len();

            for _ in 0..number_to_draw.min(self.bag_of_tile.len()) {
                let tile = self.bag_of_tile.pop().unwrap();
                hand.push(tile);
            }
        }
    }

    fn next_player(&mut self) {
        self.currently_playing += 1;
        self.currently_playing %= self.color_cycle.len();
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn is_game_over(&self) -> bool {
        self.bag_of_tile.is_empty() && self.hands.values().all(|h| h.is_empty())
    }

    pub fn bag_count(&self) -> usize {
        self.bag_of_tile.len()
    }

    pub fn playing_colors(&self) -> &[Color] {
        &self.color_cycle
    }

    pub fn skip_turn(&mut self) {
        self.next_player();
        self.current_moves = None;
    }
}

#[cfg(test)]
mod tests {

    use crate::{Board, Color, Direction, Game, Move, Player, Position, Rotation, Tile};

    /// Get the neighbor of a position in a given direction.
    fn neighbor(pos: Position, dir: Direction) -> Position {
        pos.around().find(|(_, d)| *d == dir).unwrap().0
    }

    #[test]
    fn it_works() {
        let mut game = Game::new(Player::Someone, Player::Someone, Player::None, Player::None);

        let active_color = game.currently_playing();
        let _player_tiles = game.get_hand(active_color);
        let possible_moves = game.get_possible_moves();
        let m = possible_moves[0];
        dbg!(&m);
        dbg!(&game);
        game.play_move(m).unwrap();

        for tile in enum_iterator::all::<Tile>() {
            for color in tile.colors() {
                let num_occurences = tile.colors().iter().filter(|c| *c == color).count();
                assert_eq!(
                    num_occurences, 2,
                    "color count check failed for tile {:?}",
                    tile
                )
            }
            let mut iter = enum_iterator::all::<Tile>();
            iter.any(|t| t == tile);

            for other_tile in iter {
                assert_ne!(
                    tile.colors(),
                    other_tile.colors(),
                    "different colors failed for tiles {:?} and {:?}",
                    tile,
                    other_tile
                )
            }
        }

        assert_eq!(Direction::Left.oposite(), Direction::Right);
        assert_eq!(Direction::Right.oposite(), Direction::Left);
        assert_eq!(Direction::TopLeft.oposite(), Direction::BottomRight);

        assert_eq!(Direction::Left - Rotation::R60, Direction::BottomLeft);
        assert_eq!(Direction::BottomLeft + Rotation::R60, Direction::Left);
        assert_eq!(Direction::BottomLeft - Rotation::R120, Direction::Right);

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

    #[test]
    fn rule_no_same_color_triple() {
        // Place 2 tiles around CENTER with Blue edges pointing into CENTER.
        // A 3rd tile completing the triple should be rejected.
        //
        // BYYBRR at Left(-1,0), Rotation::None: Right edge = B
        // RYYRBB at TopLeft(0,1), Rotation::None: BottomRight edge = B
        let mut board = Board::default();
        let left = neighbor(Position::CENTER, Direction::Left);
        let top_left = neighbor(Position::CENTER, Direction::TopLeft);
        let top_right = neighbor(Position::CENTER, Direction::TopRight);

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
        let left = neighbor(Position::CENTER, Direction::Left);
        let top_left = neighbor(Position::CENTER, Direction::TopLeft);
        let top_right = neighbor(Position::CENTER, Direction::TopRight);
        let right = neighbor(Position::CENTER, Direction::Right);

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

        let countour = board.countour_positions();
        // Placing at Right would give CENTER a 4th neighbor → illegal (controlled side)
        let controlled = board.controlled_positions(&countour);
        assert!(
            controlled.contains(&right),
            "right should be on a controlled side (CENTER is forced with 3 neighbors)"
        );

        // Placing far away from CENTER should be fine
        let far = neighbor(neighbor(left, Direction::Left), Direction::Left);
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
        let left = neighbor(Position::CENTER, Direction::Left);
        let top_left = neighbor(Position::CENTER, Direction::TopLeft);
        let top_right = neighbor(Position::CENTER, Direction::TopRight);
        let right = neighbor(Position::CENTER, Direction::Right);

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

        let countour = board.countour_positions();
        // CENTER is now a forced space (3 neighbors).
        // Right is adjacent to CENTER and on the contour → controlled side → should be blocked
        let controlled = board.controlled_positions(&countour);
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
