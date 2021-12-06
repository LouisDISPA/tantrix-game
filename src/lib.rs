#![feature(bool_to_option)]

use std::collections::BTreeSet;

use arrayvec::ArrayVecCopy as ArrayVec;
use enum_iterator::IntoEnumIterator;
use enum_map::{enum_map, EnumMap};
use rand::{prelude::SliceRandom, thread_rng};

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
    fn color(&self, dir: Direction) -> Color {
        self.tile.color(dir - self.rotation)
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct Board {
    moves: ArrayVec<Move, NUMBER_OF_TILE>,
}

impl Board {
    fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    fn is_available(&self, pos: &Position) -> bool {
        self.moves
            .binary_search_by(|m| m.position.cmp(pos))
            .is_err()
    }

    fn insert_move(&mut self, m: Move) -> Result<(), String> {
        if !self.is_available(&m.position) {
            return Err("Position unavalable".to_string());
        }

        let index = self
            .moves
            .binary_search_by(|i| i.position.cmp(&m.position))
            .unwrap_err();

        self.moves.try_insert(index, m).map_err(|e| e.to_string())
    }

    fn possition_is_forced(&self, pos: &Position) -> bool {
        let mut neighbor_count: u8 = 0;
        for is_available in pos.around().map(|(pos, _)| self.is_available(&pos)) {
            if !is_available {
                neighbor_count += 1;
                if neighbor_count >= 3 {
                    return true;
                }
            }
        }
        false
    }

    fn moves_for_tile(&self, tile: Tile) -> Vec<Move> {
        if self.is_empty() {
            let position = Position::CENTER;
            Rotation::into_enum_iter()
                .map(|rotation| Move {
                    tile,
                    rotation,
                    position,
                    forced: self.possition_is_forced(&position),
                })
                .collect()
        } else {
            let possible_positions = self.possible_positions();

            possible_positions
                .into_iter()
                .flat_map(|pos| self.get_possible_move(tile, pos))
                .collect()
        }
    }

    fn get_move(&self, pos: &Position) -> Option<Move> {
        match self.moves.binary_search_by(|m| m.position.cmp(pos)) {
            Ok(index) => Some(self.moves[index]),
            Err(_) => None,
        }
    }

    fn possible_positions(&self) -> BTreeSet<Position> {
        self.moves
            .iter()
            .map(|m| m.position.around())
            .flatten()
            .filter_map(|(pos, _)| self.is_available(&pos).then(|| pos))
            .collect()
    }

    fn get_possible_move(&self, tile: Tile, position: Position) -> Vec<Move> {
        let mut rotation_possible: EnumMap<Rotation, bool> = enum_map! {_ => true};

        let around_colors = position
            .around()
            .filter_map(|(pos, dir)| self.get_move(&pos).map(|m| (dir, m.color(dir.oposite()))));

        for (color_position, color) in around_colors {
            for piece_rotation in Rotation::into_enum_iter() {
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
            .map(|rotation| Move {
                tile,
                rotation,
                position,
                forced: self.possition_is_forced(&position),
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    Someone,
    Ai,
    None,
}

const TILES_IN_HAND: usize = 5;
const NUMBER_OF_COLOR: usize = Color::VARIANT_COUNT;
const NUMBER_OF_TILE: usize = Tile::VARIANT_COUNT;

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
        for i in Tile::into_enum_iter() {
            bag_of_tile.push(i)
        }
        let mut rng = thread_rng();
        bag_of_tile.shuffle(&mut rng);

        let mut players = enum_map! {
            _ => Player::None
        };
        players[Color::Red] = red;
        players[Color::Bleu] = blue;
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
            for _ in 1..TILES_IN_HAND {
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
            let mut moves: Vec<Move> = self
                .get_hand(current_player)
                .iter()
                .copied()
                .map(|tile| self.board().moves_for_tile(tile))
                .flatten()
                .collect();
            if moves.iter().any(|m| m.forced) {
                moves = moves.into_iter().filter(|m| m.forced).collect();
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

            for _ in 1..number_to_draw.min(self.bag_of_tile.len()) {
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
}

#[cfg(test)]
mod tests {

    use enum_iterator::IntoEnumIterator;

    use crate::{Color, Direction, Game, Move, Player, Position, Rotation, Tile};

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

        for tile in Tile::into_enum_iter() {
            for color in tile.into_colors_array() {
                let num_occurences = tile
                    .into_colors_array()
                    .iter()
                    .filter(|c| *c == color)
                    .count();
                assert_eq!(
                    num_occurences, 2,
                    "color count check failed for tile {:?}",
                    tile
                )
            }
            let mut iter = Tile::into_enum_iter();
            iter.any(|t| t == tile);

            for other_tile in iter {
                assert_ne!(
                    tile.into_colors_array(),
                    other_tile.into_colors_array(),
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
        assert_eq!(m.color(Direction::Left), Color::Bleu);
        assert_eq!(m.color(Direction::TopLeft), Color::Yellow);
        assert_eq!(m.color(Direction::TopRight), Color::Yellow);
        assert_eq!(m.color(Direction::Right), Color::Bleu);
        assert_eq!(m.color(Direction::BottomRight), Color::Red);
        assert_eq!(m.color(Direction::BottomLeft), Color::Red);

        let m = Move {
            tile: Tile::BYYBRR,
            position: Position::CENTER,
            rotation: Rotation::R120,
            forced: false,
        };
        assert_eq!(m.color(Direction::TopRight), Color::Bleu);
        assert_eq!(m.color(Direction::Right), Color::Yellow);
        assert_eq!(m.color(Direction::BottomRight), Color::Yellow);
        assert_eq!(m.color(Direction::BottomLeft), Color::Bleu);
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
        assert_eq!(m.color(Direction::TopRight), Color::Bleu);
        assert_eq!(m.color(Direction::Right), Color::Bleu);
        assert_eq!(m.color(Direction::BottomRight), Color::Red);
    }
}
