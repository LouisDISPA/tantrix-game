use std::collections::BTreeSet;
use std::fmt;
use std::ops::Deref;

use arrayvec::ArrayVec;
use enum_map::enum_map;
pub use enum_map::EnumMap;
use rand::prelude::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

mod coordinate_system;
pub use coordinate_system::*;

mod tiles;
pub use tiles::*;

// --- AI trait ---

pub trait TantrixAi: Send + Sync {
    fn name(&self) -> &str;
    fn clone_box(&self) -> Box<dyn TantrixAi>;
    fn choose_move(
        &self,
        board: &Board,
        hands: &EnumMap<Color, Hand>,
        current_player: Color,
        active_players: &[Color],
        possible_moves: &[Move],
    ) -> Move;
}

impl Clone for Box<dyn TantrixAi> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerScore {
    pub longest_line: usize,
    pub longest_loop: usize,
    pub score: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ScoringPath {
    pub positions: Vec<Position>,
    pub is_loop: bool,
    pub score: usize,
}

/// Find the two directions of a given color on a placed tile.
/// Returns `None` if the tile doesn't have that color.
fn color_link(m: &Move, color: Color) -> Option<(Direction, Direction)> {
    let mut dirs: ArrayVec<Direction, 2> = ArrayVec::new();
    for dir in enum_iterator::all::<Direction>() {
        if m.color(dir) == color {
            dirs.push(dir);
        }
    }
    if dirs.len() == 2 {
        Some((dirs[0], dirs[1]))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Move {
    pub tile: Tile,
    pub position: Position,
    pub rotation: Rotation,
    pub forced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveError {
    NoLegalMoves,
    TileNotInHand,
    InvalidMove,
    BoardFull,
}

impl fmt::Display for MoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoveError::NoLegalMoves => write!(f, "No legal moves available"),
            MoveError::TileNotInHand => write!(f, "Tile not in hand"),
            MoveError::InvalidMove => write!(f, "Invalid move"),
            MoveError::BoardFull => write!(f, "Board is full"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayedMove {
    pub m: Move,
    pub hand_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupError {
    NotEnoughPlayers,
    EmptyTurnCycle,
    CurrentPlayerNotInCycle,
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetupError::NotEnoughPlayers => write!(f, "A game needs at least two players"),
            SetupError::EmptyTurnCycle => write!(f, "Turn cycle cannot be empty"),
            SetupError::CurrentPlayerNotInCycle => write!(f, "Current player is not in turn cycle"),
        }
    }
}

impl Move {
    pub fn color(&self, dir: Direction) -> Color {
        self.tile.color(dir - self.rotation)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    moves: ArrayVec<Move, NUMBER_OF_TILE>,
}

impl Deref for Board {
    type Target = [Move];

    fn deref(&self) -> &Self::Target {
        &self.moves
    }
}

impl Board {
    pub fn is_available(&self, pos: &Position) -> bool {
        self.moves
            .binary_search_by(|m| m.position.cmp(pos))
            .is_err()
    }

    pub fn insert_move(&mut self, m: Move) -> Result<(), String> {
        let index = match self.moves.binary_search_by(|i| i.position.cmp(&m.position)) {
            Ok(_) => return Err("Position unavailable".to_string()),
            Err(i) => i,
        };
        self.moves.try_insert(index, m).map_err(|e| e.to_string())
    }

    fn neighbor_count(&self, pos: &Position) -> u8 {
        pos.around().filter(|(p, _)| !self.is_available(p)).count() as u8
    }

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

    fn get_move(&self, pos: &Position) -> Option<Move> {
        match self.moves.binary_search_by(|m| m.position.cmp(pos)) {
            Ok(index) => Some(self.moves[index]),
            Err(_) => None,
        }
    }

    /// Trace a color path starting from `start_pos`, exiting through `exit_dir`.
    /// Returns (count of additional tiles beyond start, whether it looped back to start).
    fn trace(
        &self,
        color: Color,
        start_pos: Position,
        exit_dir: Direction,
        visited: &mut BTreeSet<Position>,
    ) -> (usize, bool) {
        let mut count = 0;
        let mut current_pos = start_pos;
        let mut current_exit = exit_dir;

        loop {
            let next_pos = current_pos.neighbor(current_exit);
            if next_pos == start_pos {
                return (count, true);
            }
            let Some(next_move) = self.get_move(&next_pos) else {
                return (count, false);
            };
            let Some((da, db)) = color_link(&next_move, color) else {
                return (count, false);
            };
            let arrival = current_exit.opposite();
            let next_exit = if da == arrival {
                db
            } else if db == arrival {
                da
            } else {
                return (count, false);
            };

            visited.insert(next_pos);
            count += 1;
            current_pos = next_pos;
            current_exit = next_exit;
        }
    }

    /// Trace a color path, collecting positions along the way.
    /// Returns (positions beyond start in traversal order, is_loop).
    fn trace_with_positions(
        &self,
        color: Color,
        start_pos: Position,
        exit_dir: Direction,
        visited: &mut BTreeSet<Position>,
    ) -> (Vec<Position>, bool) {
        let mut positions = Vec::new();
        let mut current_pos = start_pos;
        let mut current_exit = exit_dir;

        loop {
            let next_pos = current_pos.neighbor(current_exit);
            if next_pos == start_pos {
                return (positions, true);
            }
            let Some(next_move) = self.get_move(&next_pos) else {
                return (positions, false);
            };
            let Some((da, db)) = color_link(&next_move, color) else {
                return (positions, false);
            };
            let arrival = current_exit.opposite();
            let next_exit = if da == arrival {
                db
            } else if db == arrival {
                da
            } else {
                return (positions, false);
            };

            visited.insert(next_pos);
            positions.push(next_pos);
            current_pos = next_pos;
            current_exit = next_exit;
        }
    }

    /// Compute the best scoring path for each color, including positions.
    pub fn scoring_paths(&self) -> EnumMap<Color, ScoringPath> {
        let mut visited: EnumMap<Color, BTreeSet<Position>> = enum_map! { _ => BTreeSet::new() };
        let mut best: EnumMap<Color, ScoringPath> = enum_map! { _ => ScoringPath::default() };

        for m in self.moves.iter() {
            for color in enum_iterator::all::<Color>() {
                if visited[color].contains(&m.position) {
                    continue;
                }
                let Some((dir_a, dir_b)) = color_link(m, color) else {
                    continue;
                };

                visited[color].insert(m.position);

                let (fwd_positions, is_loop) =
                    self.trace_with_positions(color, m.position, dir_a, &mut visited[color]);

                if is_loop {
                    let count = fwd_positions.len() + 1;
                    let score = count * 2;
                    if score > best[color].score {
                        let mut positions = vec![m.position];
                        positions.extend(fwd_positions);
                        best[color] = ScoringPath {
                            positions,
                            is_loop: true,
                            score,
                        };
                    }
                    continue;
                }

                let (bwd_positions, _) =
                    self.trace_with_positions(color, m.position, dir_b, &mut visited[color]);

                let total = fwd_positions.len() + bwd_positions.len() + 1;
                if total > best[color].score {
                    let mut positions: Vec<Position> = bwd_positions.into_iter().rev().collect();
                    positions.push(m.position);
                    positions.extend(fwd_positions);
                    best[color] = ScoringPath {
                        positions,
                        is_loop: false,
                        score: total,
                    };
                }
            }
        }

        // Final score: max(line, loop*2) — but we already stored the best path by score
        best
    }

    /// Compute scores for all 4 colors in a single pass over the board.
    pub fn scores(&self) -> EnumMap<Color, PlayerScore> {
        let mut visited: EnumMap<Color, BTreeSet<Position>> = enum_map! { _ => BTreeSet::new() };
        let mut best_line: EnumMap<Color, usize> = enum_map! { _ => 0 };
        let mut best_loop: EnumMap<Color, usize> = enum_map! { _ => 0 };

        for m in self.moves.iter() {
            // Each tile has 3 colors; process all of them
            for color in enum_iterator::all::<Color>() {
                if visited[color].contains(&m.position) {
                    continue;
                }
                let Some((dir_a, dir_b)) = color_link(m, color) else {
                    continue;
                };

                visited[color].insert(m.position);

                let (fwd_count, is_loop) =
                    self.trace(color, m.position, dir_a, &mut visited[color]);
                if is_loop {
                    best_loop[color] = best_loop[color].max(fwd_count + 1);
                    continue;
                }

                let (bwd_count, _) = self.trace(color, m.position, dir_b, &mut visited[color]);
                let total = fwd_count + bwd_count + 1;
                best_line[color] = best_line[color].max(total);
            }
        }

        enum_map! { color =>
            PlayerScore {
                longest_line: best_line[color],
                longest_loop: best_loop[color],
                score: best_line[color].max(best_loop[color] * 2),
            }
        }
    }

    pub fn contour_positions(&self) -> BTreeSet<Position> {
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

/// Play `m` for the current player, draw a replacement tile, and pass the
/// turn once it is over.
///
/// A turn has three steps: fill forced spaces, make one free move, then fill
/// forced spaces again. The player keeps the turn until the free move is
/// played and no forced space is left that they can fill.
pub fn play_move_parts(
    board: &mut Board,
    hands: &mut EnumMap<Color, Hand>,
    bag: &mut Bag,
    turn: &mut TurnTracker,
    m: Move,
) -> Result<PlayedMove, MoveError> {
    let hand = &mut hands[turn.current()];
    if !hand.contains(&m.tile) {
        return Err(MoveError::TileNotInHand);
    }

    let legal_moves = board.possible_moves(hand, bag.is_empty(), turn.step);
    if legal_moves.is_empty() {
        return Err(MoveError::NoLegalMoves);
    }

    let Some(canonical) = legal_moves.into_iter().find(|legal| {
        legal.tile == m.tile && legal.position == m.position && legal.rotation == m.rotation
    }) else {
        return Err(MoveError::InvalidMove);
    };

    board
        .insert_move(canonical)
        .map_err(|_| MoveError::BoardFull)?;
    let hand_index = hand.play_tile(canonical.tile, bag);

    // The one move of a turn that isn't in a forced space is the free move.
    if !canonical.forced {
        turn.step = TurnStep::AfterFreeMove;
    }
    if turn_is_over(board, hand, bag, turn.step) {
        turn.next();
    }

    Ok(PlayedMove {
        m: canonical,
        hand_index,
    })
}

/// A turn is over once the free move is played and the player can fill no
/// more forced spaces.
fn turn_is_over(board: &Board, hand: &Hand, bag: &Bag, step: TurnStep) -> bool {
    step == TurnStep::AfterFreeMove && board.possible_moves(hand, bag.is_empty(), step).is_empty()
}

pub enum Player {
    Someone,
    Ai(Box<dyn TantrixAi>),
    None,
}

impl Clone for Player {
    fn clone(&self) -> Self {
        match self {
            Player::Someone => Player::Someone,
            Player::Ai(ai) => Player::Ai(ai.clone_box()),
            Player::None => Player::None,
        }
    }
}

impl PartialEq for Player {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Player::Someone, Player::Someone)
                | (Player::Ai(_), Player::Ai(_))
                | (Player::None, Player::None)
        )
    }
}

impl Eq for Player {}

impl fmt::Debug for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Player::Someone => write!(f, "Someone"),
            Player::Ai(ai) => write!(f, "Ai({})", ai.name()),
            Player::None => write!(f, "None"),
        }
    }
}

pub const TILES_IN_HAND: usize = 6;
const NUMBER_OF_COLOR: usize = enum_iterator::cardinality::<Color>();
const NUMBER_OF_TILE: usize = enum_iterator::cardinality::<Tile>();

// --- Bag ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bag {
    tiles: ArrayVec<Tile, NUMBER_OF_TILE>,
}

impl Bag {
    pub fn new() -> Self {
        Self::new_with_rng(&mut rand::rng())
    }

    /// A full bag shuffled by `rng`; a seeded `rng` always gives the same draws.
    pub fn new_with_rng<R: Rng + ?Sized>(rng: &mut R) -> Self {
        let mut tiles: ArrayVec<Tile, NUMBER_OF_TILE> = Default::default();
        for t in enum_iterator::all::<Tile>() {
            tiles.push(t);
        }
        tiles.shuffle(rng);
        Bag { tiles }
    }

    pub fn draw(&mut self) -> Option<Tile> {
        self.tiles.pop()
    }

    pub fn count(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Build a bag with `n` placeholder tiles. The contents are arbitrary —
    /// this constructor exists so remote clients can mirror the host's
    /// `bag_count` without seeing the real draw order.
    pub fn with_count(n: usize) -> Self {
        let mut tiles: ArrayVec<Tile, NUMBER_OF_TILE> = Default::default();
        for _ in 0..n.min(NUMBER_OF_TILE) {
            tiles.push(Tile::RYYBRB);
        }
        Bag { tiles }
    }
}

impl Default for Bag {
    fn default() -> Self {
        Self::new()
    }
}

// --- Hand ---

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hand {
    tiles: ArrayVec<Tile, TILES_IN_HAND>,
}

impl Hand {
    /// Remove a tile from hand and draw a replacement from the bag.
    /// Returns the index where the tile was removed.
    pub fn play_tile(&mut self, tile: Tile, bag: &mut Bag) -> usize {
        let index = self
            .tiles
            .iter()
            .position(|t| *t == tile)
            .expect("Can't play tile not in hand");
        self.tiles.remove(index);
        // Draw replacements to fill back up
        let to_draw = TILES_IN_HAND - self.tiles.len();
        for i in 0..to_draw.min(bag.tiles.len()) {
            let new_tile = bag.draw().unwrap();
            let idx = (index + i).min(self.tiles.len());
            self.tiles.insert(idx, new_tile);
        }
        index
    }

    /// Deal `count` tiles from the bag into this hand.
    pub fn deal(&mut self, bag: &mut Bag, count: usize) {
        for _ in 0..count {
            if let Some(tile) = bag.draw() {
                self.tiles.push(tile);
            }
        }
    }
}

impl Deref for Hand {
    type Target = [Tile];

    fn deref(&self) -> &Self::Target {
        &self.tiles
    }
}

// --- TurnTracker ---

/// Where the current player is within their turn: fill forced spaces, make
/// one free move, then fill forced spaces again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TurnStep {
    /// Forced spaces come first, then the free move.
    #[default]
    BeforeFreeMove,
    /// The free move is played; only forced spaces can still be filled.
    AfterFreeMove,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnTracker {
    currently_playing: usize,
    color_cycle: ArrayVec<Color, NUMBER_OF_COLOR>,
    /// Games saved before turn steps existed resume at the turn's start.
    #[serde(default)]
    step: TurnStep,
}

impl TurnTracker {
    pub fn new(players: &EnumMap<Color, Player>) -> Result<Self, SetupError> {
        Self::new_with_rng(players, &mut rand::rng())
    }

    /// A turn order shuffled by `rng`; a seeded `rng` always gives the same order.
    pub fn new_with_rng<R: Rng + ?Sized>(
        players: &EnumMap<Color, Player>,
        rng: &mut R,
    ) -> Result<Self, SetupError> {
        let mut color_cycle: ArrayVec<Color, NUMBER_OF_COLOR> = players
            .iter()
            .filter(|(_, p)| *p != &Player::None)
            .map(|(c, _)| c)
            .collect();
        if color_cycle.is_empty() {
            return Err(SetupError::EmptyTurnCycle);
        }
        color_cycle.shuffle(rng);
        Ok(TurnTracker {
            currently_playing: 0,
            color_cycle,
            step: TurnStep::default(),
        })
    }

    pub fn current(&self) -> Color {
        self.color_cycle[self.currently_playing]
    }

    pub fn step(&self) -> TurnStep {
        self.step
    }

    pub fn next(&mut self) {
        self.currently_playing += 1;
        self.currently_playing %= self.color_cycle.len();
        self.step = TurnStep::default();
    }

    pub fn playing_colors(&self) -> &[Color] {
        &self.color_cycle
    }

    /// Build a TurnTracker pointing at `current`, at `step` of their turn,
    /// with the given ordered color cycle. Used by clients that receive an
    /// authoritative snapshot and need to reconstruct turn state without
    /// reshuffling.
    pub fn from_parts(
        current: Color,
        colors: impl IntoIterator<Item = Color>,
        step: TurnStep,
    ) -> Result<Self, SetupError> {
        let color_cycle: ArrayVec<Color, NUMBER_OF_COLOR> = colors.into_iter().collect();
        if color_cycle.is_empty() {
            return Err(SetupError::EmptyTurnCycle);
        }
        let Some(currently_playing) = color_cycle.iter().position(|&c| c == current) else {
            return Err(SetupError::CurrentPlayerNotInCycle);
        };
        Ok(TurnTracker {
            currently_playing,
            color_cycle,
            step,
        })
    }
}

// --- is_game_over free function ---

pub fn is_game_over(bag: &Bag, hands: &EnumMap<Color, Hand>) -> bool {
    bag.is_empty() && hands.values().all(|h| h.is_empty())
}

#[derive(Debug, Clone)]
pub struct Game {
    board: Board,
    bag: Bag,
    players: EnumMap<Color, Player>,
    turn_tracker: TurnTracker,
    hands: EnumMap<Color, Hand>,
    current_moves: Option<Vec<Move>>,
}

/// Complete server-side game state, including the hidden draw order. Never send
/// this over the player protocol; [`tantrix_proto::GameSnapshot`] omits the bag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameData {
    board: Board,
    bag: Bag,
    turn_tracker: TurnTracker,
    hands: EnumMap<Color, Hand>,
}

impl Game {
    /// Capture everything needed to resume an authoritative game after a
    /// server restart. Legal-move caches and AI implementations are runtime
    /// details and are deliberately excluded.
    pub fn to_data(&self) -> GameData {
        GameData {
            board: self.board.clone(),
            bag: self.bag.clone(),
            turn_tracker: self.turn_tracker.clone(),
            hands: self.hands.clone(),
        }
    }

    /// Restore a multiplayer game where every active color is human-owned.
    pub fn from_data(data: GameData) -> Self {
        let mut players = enum_map! { _ => Player::None };
        for &color in data.turn_tracker.playing_colors() {
            players[color] = Player::Someone;
        }
        Self {
            board: data.board,
            bag: data.bag,
            players,
            turn_tracker: data.turn_tracker,
            hands: data.hands,
            current_moves: None,
        }
    }

    pub fn new(
        red: Player,
        blue: Player,
        yellow: Player,
        green: Player,
    ) -> Result<Self, SetupError> {
        Self::new_with_rng(red, blue, yellow, green, &mut rand::rng())
    }

    /// A game whose bag and turn order are shuffled by `rng`; a seeded `rng`
    /// always deals the same game.
    pub fn new_with_rng<R: Rng + ?Sized>(
        red: Player,
        blue: Player,
        yellow: Player,
        green: Player,
        rng: &mut R,
    ) -> Result<Self, SetupError> {
        let board = Default::default();
        let mut bag = Bag::new_with_rng(rng);

        let mut players = enum_map! {
            _ => Player::None
        };
        players[Color::Red] = red;
        players[Color::Blue] = blue;
        players[Color::Yellow] = yellow;
        players[Color::Green] = green;

        let active_count = players.values().filter(|p| **p != Player::None).count();
        if active_count < 2 {
            return Err(SetupError::NotEnoughPlayers);
        }
        let turn_tracker = TurnTracker::new_with_rng(&players, rng)?;

        let mut hands: EnumMap<Color, Hand> = Default::default();
        for &color in turn_tracker.playing_colors() {
            hands[color].deal(&mut bag, TILES_IN_HAND);
        }

        Ok(Self {
            board,
            bag,
            players,
            turn_tracker,
            hands,
            current_moves: None,
        })
    }

    pub fn currently_playing(&self) -> Color {
        self.turn_tracker.current()
    }

    pub fn turn_step(&self) -> TurnStep {
        self.turn_tracker.step()
    }

    pub fn get_player(&self, color: Color) -> &Player {
        &self.players[color]
    }

    pub fn get_hand(&self, color: Color) -> &Hand {
        &self.hands[color]
    }

    pub fn get_possible_moves(&mut self) -> &[Move] {
        if self.current_moves.is_none() {
            let current_player = self.currently_playing();
            let moves = self.board.possible_moves(
                &self.hands[current_player],
                self.bag.is_empty(),
                self.turn_tracker.step(),
            );
            self.current_moves = Some(moves);
        }
        match &self.current_moves {
            Some(moves) => moves,
            None => unreachable!(),
        }
    }

    pub fn play_move(&mut self, m: impl Into<Move>) -> Result<(), MoveError> {
        play_move_parts(
            &mut self.board,
            &mut self.hands,
            &mut self.bag,
            &mut self.turn_tracker,
            m.into(),
        )?;
        self.current_moves = None;
        Ok(())
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn is_game_over(&self) -> bool {
        self.bag.is_empty() && self.hands.values().all(|h| h.is_empty())
    }

    pub fn bag_count(&self) -> usize {
        self.bag.count()
    }

    pub fn playing_colors(&self) -> &[Color] {
        self.turn_tracker.playing_colors()
    }

    pub fn skip_turn(&mut self) {
        self.turn_tracker.next();
        self.current_moves = None;
    }

    /// Remove `color` from the turn cycle mid-game. Used when a seated
    /// participant is kicked: their seat and hand go away, and the cycle
    /// rotates past them cleanly.
    ///
    /// Returns `Err` if the color wasn't currently playing, or if removing
    /// it would leave fewer than one player in the cycle.
    pub fn retire_color(&mut self, color: Color) -> Result<(), &'static str> {
        let cycle = &self.turn_tracker.color_cycle;
        let Some(idx) = cycle.iter().position(|&c| c == color) else {
            return Err("color is not in the turn cycle");
        };
        if cycle.len() <= 1 {
            return Err("cannot retire the last remaining player");
        }
        self.turn_tracker.color_cycle.remove(idx);
        // Keep `currently_playing` pointing at the same player, or at the
        // next one (at the start of their turn) if we just removed the
        // current index.
        if self.turn_tracker.currently_playing > idx {
            self.turn_tracker.currently_playing -= 1;
        } else {
            if self.turn_tracker.currently_playing == idx {
                self.turn_tracker.step = TurnStep::default();
            }
            if self.turn_tracker.currently_playing >= self.turn_tracker.color_cycle.len() {
                self.turn_tracker.currently_playing = 0;
            }
        }
        self.players[color] = Player::None;
        self.hands[color] = Hand::default();
        self.current_moves = None;
        Ok(())
    }

    pub fn scores(&self) -> EnumMap<Color, PlayerScore> {
        self.board.scores()
    }

    pub fn scoring_paths(&self) -> EnumMap<Color, ScoringPath> {
        self.board.scoring_paths()
    }

    pub fn winner(&self) -> Option<(Color, PlayerScore)> {
        if !self.is_game_over() {
            return None;
        }
        let scores = self.scores();
        self.playing_colors()
            .iter()
            .map(|&c| (c, scores[c]))
            .max_by_key(|(_, s)| s.score)
    }

    /// If the current player is AI, compute and return its chosen move without playing it.
    /// Returns None if the current player is human, has no moves, or the game is over.
    pub fn ai_choose(&mut self) -> Option<Move> {
        if self.is_game_over() {
            return None;
        }
        let possible = self.get_possible_moves().to_vec();
        if possible.is_empty() {
            return None;
        }
        let current = self.currently_playing();
        if let Player::Ai(ref ai) = self.players[current] {
            let board = self.board.clone();
            Some(ai.choose_move(
                &board,
                &self.hands,
                current,
                self.turn_tracker.playing_colors(),
                &possible,
            ))
        } else {
            None
        }
    }

    /// Play all AI turns and skip players with no valid moves.
    /// Returns when it is a human player's turn or the game is over.
    pub fn advance(&mut self) {
        let num_players = self.turn_tracker.playing_colors().len();
        let mut consecutive_skips = 0;

        loop {
            if self.is_game_over() || consecutive_skips >= num_players {
                return;
            }

            if let Some(m) = self.ai_choose() {
                consecutive_skips = 0;
                self.play_move(m).expect("AI chose invalid move");
            } else if !self.get_possible_moves().is_empty() {
                return; // Human player with moves
            } else {
                self.skip_turn();
                consecutive_skips += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use rand::rngs::Xoshiro256PlusPlus;
    use rand::SeedableRng;

    use crate::{
        Board, Color, Direction, Game, Move, MoveError, Player, Position, Rotation, Tile, TurnStep,
        TurnTracker,
    };

    #[test]
    fn persisted_game_keeps_bag_and_turn() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        let first = game.get_possible_moves()[0];
        game.play_move(first).unwrap();

        let encoded = serde_json::to_vec(&game.to_data()).unwrap();
        let restored = Game::from_data(serde_json::from_slice(&encoded).unwrap());
        assert_eq!(restored.board(), game.board());
        assert_eq!(restored.bag_count(), game.bag_count());
        assert_eq!(restored.currently_playing(), game.currently_playing());
        for &color in game.playing_colors() {
            assert_eq!(restored.get_hand(color), game.get_hand(color));
        }
        assert!(!encoded.is_empty());
    }

    /// Get the neighbor of a position in a given direction.
    fn neighbor(pos: Position, dir: Direction) -> Position {
        pos.around().find(|(_, d)| *d == dir).unwrap().0
    }

    #[test]
    fn it_works() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();

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

        assert_eq!(Direction::Left.opposite(), Direction::Right);
        assert_eq!(Direction::Right.opposite(), Direction::Left);
        assert_eq!(Direction::TopLeft.opposite(), Direction::BottomRight);

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

        let contour = board.contour_positions();
        // Placing at Right would give CENTER a 4th neighbor → illegal (controlled side)
        let controlled = board.controlled_positions(&contour);
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

    #[test]
    fn test_color_link() {
        use crate::color_link;

        // BYYBRR = [B, Y, Y, B, R, R], no rotation
        let m = Move {
            tile: Tile::BYYBRR,
            position: Position::CENTER,
            rotation: Rotation::None,
            forced: false,
        };
        // Blue at Left(0) and Right(3)
        assert_eq!(
            color_link(&m, Color::Blue),
            Some((Direction::Left, Direction::Right))
        );
        // Yellow at TopLeft(1) and TopRight(2)
        assert_eq!(
            color_link(&m, Color::Yellow),
            Some((Direction::TopLeft, Direction::TopRight))
        );
        // Red at BottomRight(4) and BottomLeft(5)
        assert_eq!(
            color_link(&m, Color::Red),
            Some((Direction::BottomRight, Direction::BottomLeft))
        );
        // Green not present
        assert_eq!(color_link(&m, Color::Green), None);
    }

    #[test]
    fn test_single_tile_score() {
        // One tile on the board: line of 1 for each of its colors
        let mut board = Board::default();
        board
            .insert_move(Move {
                tile: Tile::BYYBRR,
                position: Position::CENTER,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();

        let blue_score = board.scores()[Color::Blue];
        assert_eq!(blue_score.longest_line, 1);
        assert_eq!(blue_score.longest_loop, 0);
        assert_eq!(blue_score.score, 1);

        // Green not on this tile
        let green_score = board.scores()[Color::Green];
        assert_eq!(green_score.longest_line, 0);
        assert_eq!(green_score.score, 0);

        // scoring_paths should return positions for blue
        let paths = board.scoring_paths();
        assert_eq!(paths[Color::Blue].positions.len(), 1);
        assert_eq!(paths[Color::Blue].positions[0], Position::CENTER);
        assert_eq!(paths[Color::Blue].score, 1);
        // Green has no path
        assert!(paths[Color::Green].positions.is_empty());
        assert_eq!(paths[Color::Green].score, 0);
    }

    #[test]
    fn test_two_tile_line() {
        // BYYBRR at CENTER: [B,Y,Y,B,R,R] → Right=Blue
        // BYRBRY at Right(1,0): [B,Y,R,B,R,Y] → Left=Blue
        // Blue connects through Right↔Left edge
        let mut board = Board::default();
        let right = neighbor(Position::CENTER, Direction::Right);

        board
            .insert_move(Move {
                tile: Tile::BYYBRR,
                position: Position::CENTER,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();
        board
            .insert_move(Move {
                tile: Tile::BYRBRY,
                position: right,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();

        let blue_score = board.scores()[Color::Blue];
        assert_eq!(blue_score.longest_line, 2);
        assert_eq!(blue_score.longest_loop, 0);
        assert_eq!(blue_score.score, 2);
    }

    #[test]
    fn test_three_tile_line() {
        // RYYBRB at (-1,0): [R,Y,Y,B,R,B] → Right=Blue
        // BYYBRR at (0,0): [B,Y,Y,B,R,R] → Left=Blue, Right=Blue
        // BYRBRY at (1,0): [B,Y,R,B,R,Y] → Left=Blue
        // Blue line: RYYBRB → BYYBRR → BYRBRY = 3 tiles
        let mut board = Board::default();
        let left = neighbor(Position::CENTER, Direction::Left);
        let right = neighbor(Position::CENTER, Direction::Right);

        board
            .insert_move(Move {
                tile: Tile::RYYBRB,
                position: left,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();
        board
            .insert_move(Move {
                tile: Tile::BYYBRR,
                position: Position::CENTER,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();
        board
            .insert_move(Move {
                tile: Tile::BYRBRY,
                position: right,
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();

        let blue_score = board.scores()[Color::Blue];
        assert_eq!(blue_score.longest_line, 3);
        assert_eq!(blue_score.longest_loop, 0);
        assert_eq!(blue_score.score, 3);
    }

    #[test]
    fn test_loop_detection() {
        // 6 tiles forming a Red loop around CENTER (CENTER is empty).
        // Ring: (-1,0) → (0,1) → (1,1) → (1,0) → (0,-1) → (-1,-1) → back
        let mut board = Board::default();

        let pos = [
            neighbor(Position::CENTER, Direction::Left),     // (-1,0)
            neighbor(Position::CENTER, Direction::TopLeft),  // (0,1)
            neighbor(Position::CENTER, Direction::TopRight), // (1,1)
            neighbor(Position::CENTER, Direction::Right),    // (1,0)
            neighbor(Position::CENTER, Direction::BottomRight), // (0,-1)
            neighbor(Position::CENTER, Direction::BottomLeft), // (-1,-1)
        ];

        // Each tile placed so Red edges connect to its ring neighbors:
        // (-1,0): Red at TopRight & BottomRight
        board
            .insert_move(Move {
                tile: Tile::BYRBRY, // [B,Y,R,B,R,Y] → R at 2,4
                position: pos[0],
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();

        // (0,1): Red at BottomLeft & Right
        board
            .insert_move(Move {
                tile: Tile::YBBRYR, // [Y,B,B,R,Y,R] → R at 3,5
                position: pos[1],
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();

        // (1,1): Red at Left & BottomRight
        board
            .insert_move(Move {
                tile: Tile::RYYBRB, // [R,Y,Y,B,R,B] → R at 0,4
                position: pos[2],
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();

        // (1,0): Red at TopLeft & BottomLeft
        board
            .insert_move(Move {
                tile: Tile::YRBYBR, // [Y,R,B,Y,B,R] → R at 1,5
                position: pos[3],
                rotation: Rotation::None,
                forced: false,
            })
            .unwrap();

        // (0,-1): Red at Left & TopRight. YGRYRG [Y,G,R,Y,R,G] R at 2,4 + R240 → (0,2)
        board
            .insert_move(Move {
                tile: Tile::YGRYRG,
                position: pos[4],
                rotation: Rotation::R240,
                forced: false,
            })
            .unwrap();

        // (-1,-1): Red at TopLeft & Right. GYYRGR [G,Y,Y,R,G,R] R at 3,5 + R240 → (1,3)
        board
            .insert_move(Move {
                tile: Tile::GYYRGR,
                position: pos[5],
                rotation: Rotation::R240,
                forced: false,
            })
            .unwrap();

        let red_score = board.scores()[Color::Red];
        assert_eq!(red_score.longest_loop, 6);
        assert_eq!(red_score.longest_line, 0);
        assert_eq!(red_score.score, 12); // 6 * 2
    }

    #[test]
    fn game_play_move_rejects_illegal_rotation() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        let first = game.get_possible_moves()[0];
        game.play_move(first).unwrap();

        let possible = game.get_possible_moves().to_vec();
        let illegal = possible
            .iter()
            .find_map(|m| {
                enum_iterator::all::<Rotation>()
                    .find(|&rotation| {
                        !possible.iter().any(|legal| {
                            legal.tile == m.tile
                                && legal.position == m.position
                                && legal.rotation == rotation
                        })
                    })
                    .map(|rotation| Move { rotation, ..*m })
            })
            .expect("post-opening moves should include a constrained position");

        assert_eq!(game.play_move(illegal), Err(MoveError::InvalidMove));
    }

    #[test]
    fn game_new_rejects_less_than_two_players() {
        assert_eq!(
            Game::new(Player::Someone, Player::None, Player::None, Player::None).unwrap_err(),
            crate::SetupError::NotEnoughPlayers
        );
    }

    #[test]
    fn test_winner_not_over() {
        let game = Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        assert_eq!(game.winner(), None);
    }

    #[test]
    fn retire_color_drops_from_cycle() {
        let mut game = Game::new(
            Player::Someone,
            Player::Someone,
            Player::Someone,
            Player::None,
        )
        .unwrap();
        let initial = game.playing_colors().to_vec();
        assert_eq!(initial.len(), 3);
        let retire = initial[1];

        game.retire_color(retire).unwrap();

        let remaining = game.playing_colors().to_vec();
        assert_eq!(remaining.len(), 2);
        assert!(!remaining.contains(&retire));
        assert_eq!(*game.get_player(retire), Player::None);
        assert!(game.get_hand(retire).is_empty());
    }

    #[test]
    fn retire_color_advances_current_correctly() {
        // Force a deterministic cycle by retiring only colors not currently
        // in play: retiring the *current* player should roll to the next.
        let mut game = Game::new(
            Player::Someone,
            Player::Someone,
            Player::Someone,
            Player::None,
        )
        .unwrap();
        let current_before = game.currently_playing();
        game.retire_color(current_before).unwrap();
        let current_after = game.currently_playing();
        assert_ne!(current_after, current_before);
        assert!(game.playing_colors().contains(&current_after));
    }

    #[test]
    fn retire_color_rejects_last_player() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        let first = game.playing_colors()[0];
        game.retire_color(first).unwrap();
        let last = game.playing_colors()[0];
        assert!(game.retire_color(last).is_err());
    }

    #[test]
    fn retire_color_rejects_unknown_color() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        // Find a color not in the cycle — at least one of the four must be absent.
        let absent = [Color::Red, Color::Blue, Color::Yellow, Color::Green]
            .into_iter()
            .find(|c| !game.playing_colors().contains(c))
            .expect("2-player game always has two absent colors");
        assert!(game.retire_color(absent).is_err());
    }

    // --- Turn structure: forced spaces, one free move, forced spaces again ---

    /// A two-player game dealt from `seed`.
    fn seeded_game(seed: u64) -> Game {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        Game::new_with_rng(
            Player::Someone,
            Player::Someone,
            Player::None,
            Player::None,
            &mut rng,
        )
        .unwrap()
    }

    /// Plays the first legal move of seeded deals until `found` holds, and
    /// returns that game. Deterministic: the same seeds always give the same game.
    fn seeded_game_where(found: impl Fn(&mut Game) -> bool) -> Game {
        for seed in 0..100 {
            let mut game = seeded_game(seed);
            loop {
                if found(&mut game) {
                    return game;
                }
                let Some(&m) = game.get_possible_moves().first() else {
                    break;
                };
                game.play_move(m).unwrap();
            }
        }
        panic!("no seeded game reached the wanted state");
    }

    fn after_free_move(game: &mut Game) -> bool {
        game.turn_step() == TurnStep::AfterFreeMove
    }

    /// A placement the current player could make outside every forced space,
    /// respecting the restriction rules.
    fn free_placement(game: &Game) -> Option<Move> {
        let board = game.board();
        let contour = board.contour_positions();
        let controlled = board.controlled_positions(&contour);
        game.get_hand(game.currently_playing())
            .iter()
            .find_map(|&tile| {
                contour
                    .iter()
                    .filter(|p| !board.position_is_forced(p) && !controlled.contains(p))
                    .flat_map(|&p| board.get_possible_move(tile, p))
                    .find(|m| !board.creates_same_color_triple(m))
            })
    }

    #[test]
    fn free_move_ends_turn_without_forced_spaces() {
        let mut game = seeded_game(0);
        let first = game.currently_playing();
        let opening = game.get_possible_moves()[0];
        assert!(!opening.forced);

        game.play_move(opening).unwrap();

        assert_ne!(game.currently_playing(), first);
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn forced_fill_before_free_move_keeps_the_free_move() {
        let mut game = seeded_game_where(|g| {
            g.turn_step() == TurnStep::BeforeFreeMove
                && g.get_possible_moves().first().is_some_and(|m| m.forced)
        });
        let player = game.currently_playing();

        let fill = game.get_possible_moves()[0];
        game.play_move(fill).unwrap();

        assert_eq!(game.currently_playing(), player);
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn free_move_keeps_turn_to_fill_forced_spaces() {
        let mut game = seeded_game_where(after_free_move);
        let player = game.currently_playing();

        // Only forced spaces are left: a second free move is refused.
        let moves = game.get_possible_moves().to_vec();
        assert!(!moves.is_empty());
        assert!(moves.iter().all(|m| m.forced));
        let second_free_move = free_placement(&game).expect("the hand fits a free space");
        assert_eq!(
            game.play_move(second_free_move),
            Err(MoveError::InvalidMove)
        );

        // The turn passes once no forced space can be filled.
        while game.currently_playing() == player {
            let fill = game.get_possible_moves()[0];
            assert!(fill.forced);
            game.play_move(fill).unwrap();
        }
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn seeded_games_follow_the_three_step_turn() {
        let mut turns_kept_after_free_move = 0;
        for seed in 0..20 {
            let mut game = seeded_game(seed);
            let mut stuck = 0;
            while !game.is_game_over() && stuck < game.playing_colors().len() {
                let Some(&m) = game.get_possible_moves().first() else {
                    game.skip_turn();
                    stuck += 1;
                    continue;
                };
                stuck = 0;
                let player = game.currently_playing();
                let step = game.turn_step();

                game.play_move(m).unwrap();

                // After the free move, only forced spaces may be filled.
                assert!(step == TurnStep::BeforeFreeMove || m.forced, "seed {seed}");
                let free_move_done = step == TurnStep::AfterFreeMove || !m.forced;
                let forced_left = !game
                    .board()
                    .possible_moves(
                        game.get_hand(player),
                        game.bag_count() == 0,
                        TurnStep::AfterFreeMove,
                    )
                    .is_empty();
                // The turn passes exactly when the free move is done and no
                // forced space can be filled.
                let turn_passed = game.currently_playing() != player;
                assert_eq!(turn_passed, free_move_done && !forced_left, "seed {seed}");
                if free_move_done && forced_left {
                    turns_kept_after_free_move += 1;
                }
            }
        }
        assert!(turns_kept_after_free_move > 0);
    }

    #[test]
    fn persisted_game_keeps_turn_step() {
        let mut game = seeded_game_where(after_free_move);

        let encoded = serde_json::to_vec(&game.to_data()).unwrap();
        let mut restored = Game::from_data(serde_json::from_slice(&encoded).unwrap());

        assert_eq!(restored.turn_step(), TurnStep::AfterFreeMove);
        assert_eq!(restored.get_possible_moves(), game.get_possible_moves());
    }

    #[test]
    fn saved_turn_without_step_resumes_at_turn_start() {
        let turn: TurnTracker =
            serde_json::from_str(r#"{"currently_playing":1,"color_cycle":["Red","Blue"]}"#)
                .unwrap();
        assert_eq!(turn.current(), Color::Blue);
        assert_eq!(turn.step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn next_player_starts_at_the_beginning_of_their_turn() {
        let mut skipped = seeded_game_where(after_free_move);
        skipped.skip_turn();
        assert_eq!(skipped.turn_step(), TurnStep::BeforeFreeMove);

        let mut retired = seeded_game_where(after_free_move);
        let player = retired.currently_playing();
        retired.retire_color(player).unwrap();
        assert_ne!(retired.currently_playing(), player);
        assert_eq!(retired.turn_step(), TurnStep::BeforeFreeMove);
    }
}
