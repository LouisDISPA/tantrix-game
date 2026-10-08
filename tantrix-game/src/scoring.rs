//! Scoring: each color's longest line (1 point per tile) or loop (2 points
//! per tile) on the board.

use arrayvec::ArrayVec;
use enum_map::{enum_map, EnumMap};

use crate::grid::Grid;
use crate::tiles::NUMBER_OF_TILE;
use crate::{Board, Color, Direction, Move, Position};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerScore {
    pub longest_line: usize,
    pub longest_loop: usize,
    pub score: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScoringPath {
    pub positions: Vec<Position>,
    pub is_loop: bool,
    pub score: usize,
}

/// Find the two directions of a given color on a placed tile.
/// Returns `None` if the tile doesn't have that color.
#[cfg(test)]
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

/// One line or loop of one color on the board ([`Board::lines`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line {
    pub color: Color,
    pub tiles: usize,
    pub is_loop: bool,
    /// Where each end of a line leads; `None` for a loop.
    pub ends: [Option<LineEnd>; 2],
}

/// Where a line ends: the empty position its last tile leads into. A tile
/// placed there shows the line's color `toward` the line, by the golden
/// rule, so it extends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineEnd {
    pub position: Position,
    /// From `position` to the line's last tile.
    pub toward: Direction,
}

/// Where [`Board::trace`] stops.
enum PathEnd {
    /// Back round at the start: the path is a loop.
    Loop,
    /// At an empty position.
    Open(LineEnd),
    /// At a tile that doesn't go on with the color; never on a board that
    /// follows the golden rule.
    Closed,
}

impl PathEnd {
    fn is_loop(&self) -> bool {
        matches!(self, PathEnd::Loop)
    }
}

/// One line or loop of one color, as [`Board::for_each_path`] finds it.
struct Trace {
    color: Color,
    /// The tile the path was found from.
    start: Position,
    is_loop: bool,
    /// Where a line's forward and backward ends lead.
    ends: [Option<LineEnd>; 2],
    /// The tiles after `start` through its first link of `color`, in order:
    /// for a loop, the rest of the loop.
    forward: ArrayVec<Position, NUMBER_OF_TILE>,
    /// The tiles after `start` through its second link, in order; none for
    /// a loop.
    backward: ArrayVec<Position, NUMBER_OF_TILE>,
}

impl Trace {
    fn tiles(&self) -> usize {
        1 + self.forward.len() + self.backward.len()
    }

    /// A line scores 1 point per tile, a loop 2.
    fn score(&self) -> usize {
        if self.is_loop {
            self.tiles() * 2
        } else {
            self.tiles()
        }
    }

    /// The path's positions from one end to the other (for a loop, from
    /// `start` round).
    fn positions(&self) -> Vec<Position> {
        let mut positions = Vec::with_capacity(self.tiles());
        positions.extend(self.backward.iter().rev());
        positions.push(self.start);
        positions.extend(&self.forward);
        positions
    }
}

impl Board {
    /// Follow `color` from `start` out through `exit`, calling `visit` with
    /// the board index of each tile on the way (`tile_at` finds the board
    /// index of a position's tile), to where the path stops. `start` itself
    /// need not hold a tile yet ([`Self::scores_after`]).
    fn trace(
        &self,
        tile_at: impl Fn(Position) -> Option<usize>,
        color: Color,
        start: Position,
        exit: Direction,
        mut visit: impl FnMut(usize),
    ) -> PathEnd {
        let mut current_pos = start;
        let mut current_exit = exit;

        loop {
            let next_pos = current_pos.neighbor(current_exit);
            if next_pos == start {
                return PathEnd::Loop;
            }
            let arrival = current_exit.opposite();
            let Some(next) = tile_at(next_pos) else {
                return PathEnd::Open(LineEnd {
                    position: next_pos,
                    toward: arrival,
                });
            };
            let edges = self[next].edges();
            if edges.get(arrival) != Some(color) {
                return PathEnd::Closed;
            }

            visit(next);
            current_pos = next_pos;
            current_exit = edges.other_end(arrival);
        }
    }

    /// Every line and loop on the board, once per color: tile by tile in
    /// board order, each color of the tile not already on a path found.
    fn for_each_path(&self, mut found: impl FnMut(&Trace)) {
        if self.is_empty() {
            return;
        }
        let grid = Grid::of(self);
        let tile_at = |pos| grid.tile(pos);
        // Board indices already on a path, per color (the board holds at
        // most 56 tiles).
        let mut visited: EnumMap<Color, u64> = EnumMap::default();
        let mut trace = Trace {
            color: Color::Red,
            start: Position::CENTER,
            is_loop: false,
            ends: [None; 2],
            forward: ArrayVec::new(),
            backward: ArrayVec::new(),
        };

        for (index, m) in self.iter().enumerate() {
            let edges = m.edges();
            // Each color once, from the first of its two edges.
            for dir_a in enum_iterator::all::<Direction>() {
                let dir_b = edges.other_end(dir_a);
                if dir_b.index() < dir_a.index() {
                    continue;
                }
                let color = edges.get(dir_a).expect("a tile's edges are all set");
                if visited[color] & (1 << index) != 0 {
                    continue;
                }

                let seen = &mut visited[color];
                *seen |= 1 << index;
                trace.color = color;
                trace.start = m.position;
                trace.forward.clear();
                trace.backward.clear();

                let forward = self.trace(tile_at, color, m.position, dir_a, |next| {
                    *seen |= 1 << next;
                    trace.forward.push(self[next].position);
                });
                trace.is_loop = forward.is_loop();
                trace.ends = [None; 2];
                if !trace.is_loop {
                    let backward = self.trace(tile_at, color, m.position, dir_b, |next| {
                        *seen |= 1 << next;
                        trace.backward.push(self[next].position);
                    });
                    for (end, path_end) in trace.ends.iter_mut().zip([forward, backward]) {
                        if let PathEnd::Open(line_end) = path_end {
                            *end = Some(line_end);
                        }
                    }
                }
                found(&trace);
            }
        }
    }

    /// Every line and loop on the board, of every color, each once: tile by
    /// tile in board order, each color of the tile not already on one.
    pub fn lines(&self) -> Vec<Line> {
        let mut lines = Vec::new();
        self.for_each_path(|trace| {
            lines.push(Line {
                color: trace.color,
                tiles: trace.tiles(),
                is_loop: trace.is_loop,
                ends: trace.ends,
            })
        });
        lines
    }

    /// Compute the best scoring path for each color, including positions:
    /// the first path found with the color's top score.
    pub fn scoring_paths(&self) -> EnumMap<Color, ScoringPath> {
        let mut best: EnumMap<Color, ScoringPath> = enum_map! { _ => ScoringPath::default() };
        self.for_each_path(|trace| {
            let score = trace.score();
            if score > best[trace.color].score {
                best[trace.color] = ScoringPath {
                    positions: trace.positions(),
                    is_loop: trace.is_loop,
                    score,
                };
            }
        });
        best
    }

    /// Compute scores for all 4 colors in a single pass over the board.
    pub fn scores(&self) -> EnumMap<Color, PlayerScore> {
        let mut best_line: EnumMap<Color, usize> = enum_map! { _ => 0 };
        let mut best_loop: EnumMap<Color, usize> = enum_map! { _ => 0 };
        self.for_each_path(|trace| {
            let best = if trace.is_loop {
                &mut best_loop[trace.color]
            } else {
                &mut best_line[trace.color]
            };
            *best = (*best).max(trace.tiles());
        });

        enum_map! { color =>
            PlayerScore {
                longest_line: best_line[color],
                longest_loop: best_loop[color],
                score: best_line[color].max(best_loop[color] * 2),
            }
        }
    }

    /// `color`'s score once `m` is placed, given its `score` now, for a
    /// legal move `m`: the `score` of [`Self::scores_after`] for `color`,
    /// from `color`'s path through `m` alone. A loop `m` closes outscores the
    /// line it closes (twice the tiles, and one more), so the color's score
    /// is the larger of its score now and the path through `m`.
    pub fn score_after(&self, score: usize, m: &Move, color: Color) -> usize {
        self.score_after_by(|pos| self.index_of(&pos), score, m, color)
    }

    /// [`Self::score_after`], finding tiles with `tile_at`.
    fn score_after_by(
        &self,
        tile_at: impl Fn(Position) -> Option<usize> + Copy,
        score: usize,
        m: &Move,
        color: Color,
    ) -> usize {
        let edges = m.edges();
        let Some(dir_a) = enum_iterator::all::<Direction>().find(|&d| edges.get(d) == Some(color))
        else {
            return score;
        };
        let mut tiles = 1;
        let path = if self
            .trace(tile_at, color, m.position, dir_a, |_| tiles += 1)
            .is_loop()
        {
            tiles * 2
        } else {
            let dir_b = edges.other_end(dir_a);
            self.trace(tile_at, color, m.position, dir_b, |_| tiles += 1);
            tiles
        };
        score.max(path)
    }

    /// The [`Self::scores`] of this board once `m` is placed, given this
    /// board's `scores`, for a legal move `m` (it follows the golden rule, so
    /// every path it touches runs on through it). Equal to scoring a copy of
    /// the board with `m` on it, without the copy: paths only grow when a
    /// tile is added, so each color's longest line is the longer of the old
    /// one and the new line through `m`. Only a move that closes a loop is
    /// scored on a copy, since the line it closes may have been the longest.
    pub fn scores_after(
        &self,
        scores: &EnumMap<Color, PlayerScore>,
        m: &Move,
    ) -> EnumMap<Color, PlayerScore> {
        self.scores_after_by(|pos| self.index_of(&pos), scores, m)
    }

    /// [`Self::scores_after`], finding tiles with `tile_at`.
    fn scores_after_by(
        &self,
        tile_at: impl Fn(Position) -> Option<usize> + Copy,
        scores: &EnumMap<Color, PlayerScore>,
        m: &Move,
    ) -> EnumMap<Color, PlayerScore> {
        let mut after = *scores;
        let edges = m.edges();
        for dir_a in enum_iterator::all::<Direction>() {
            let dir_b = edges.other_end(dir_a);
            if dir_b.index() < dir_a.index() {
                continue;
            }
            let color = edges.get(dir_a).expect("a tile's edges are all set");
            let mut tiles = 1;
            let closes_loop = self
                .trace(tile_at, color, m.position, dir_a, |_| tiles += 1)
                .is_loop()
                || self
                    .trace(tile_at, color, m.position, dir_b, |_| tiles += 1)
                    .is_loop();
            if closes_loop {
                let mut board = self.clone();
                board
                    .insert_move(*m)
                    .expect("a legal move goes on an empty position");
                return board.scores();
            }
            let score = &mut after[color];
            score.longest_line = score.longest_line.max(tiles);
            score.score = score.score.max(tiles);
        }
        after
    }

    /// This board with its tiles indexed by position, to score many moves
    /// on it in a row.
    pub fn indexed(&self) -> IndexedBoard<'_> {
        IndexedBoard {
            board: self,
            grid: (!self.is_empty()).then(|| Grid::of(self)),
        }
    }
}

/// A board with its tiles indexed by position ([`Board::indexed`]):
/// [`Board::score_after`] and [`Board::scores_after`] for many moves on the
/// same board, without searching the board for each tile a path runs
/// through.
pub struct IndexedBoard<'a> {
    board: &'a Board,
    /// `None` for an empty board.
    grid: Option<Grid>,
}

impl IndexedBoard<'_> {
    fn tile_at(&self) -> impl Fn(Position) -> Option<usize> + Copy + '_ {
        move |pos| self.grid.as_ref().and_then(|grid| grid.tile(pos))
    }

    /// [`Board::score_after`].
    pub fn score_after(&self, score: usize, m: &Move, color: Color) -> usize {
        self.board.score_after_by(self.tile_at(), score, m, color)
    }

    /// [`Board::scores_after`].
    pub fn scores_after(
        &self,
        scores: &EnumMap<Color, PlayerScore>,
        m: &Move,
    ) -> EnumMap<Color, PlayerScore> {
        self.board.scores_after_by(self.tile_at(), scores, m)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use enum_map::{enum_map, EnumMap};

    use super::color_link;
    use crate::test_games::for_each_seeded_state;
    use crate::{
        Board, Color, Direction, Move, PlayerScore, Position, Rotation, ScoringPath, Tile,
    };

    /// The straightforward scoring: every path traced on its own, with the
    /// positions already on a path kept in a set per color. Returns what
    /// [`Board::scores`] and [`Board::scoring_paths`] must return.
    fn score_path_by_path(
        board: &Board,
    ) -> (EnumMap<Color, PlayerScore>, EnumMap<Color, ScoringPath>) {
        fn trace(
            board: &Board,
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
                let Some(next_move) = board.get_move(&next_pos) else {
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

        let mut visited: EnumMap<Color, BTreeSet<Position>> = enum_map! { _ => BTreeSet::new() };
        let mut best_line: EnumMap<Color, usize> = enum_map! { _ => 0 };
        let mut best_loop: EnumMap<Color, usize> = enum_map! { _ => 0 };
        let mut best: EnumMap<Color, ScoringPath> = enum_map! { _ => ScoringPath::default() };
        for m in board.iter() {
            for color in enum_iterator::all::<Color>() {
                if visited[color].contains(&m.position) {
                    continue;
                }
                let Some((dir_a, dir_b)) = color_link(m, color) else {
                    continue;
                };
                visited[color].insert(m.position);
                let (fwd, is_loop) = trace(board, color, m.position, dir_a, &mut visited[color]);
                if is_loop {
                    let count = fwd.len() + 1;
                    best_loop[color] = best_loop[color].max(count);
                    if count * 2 > best[color].score {
                        let mut positions = vec![m.position];
                        positions.extend(fwd);
                        best[color] = ScoringPath {
                            positions,
                            is_loop: true,
                            score: count * 2,
                        };
                    }
                    continue;
                }
                let (bwd, _) = trace(board, color, m.position, dir_b, &mut visited[color]);
                let total = fwd.len() + bwd.len() + 1;
                best_line[color] = best_line[color].max(total);
                if total > best[color].score {
                    let mut positions: Vec<Position> = bwd.into_iter().rev().collect();
                    positions.push(m.position);
                    positions.extend(fwd);
                    best[color] = ScoringPath {
                        positions,
                        is_loop: false,
                        score: total,
                    };
                }
            }
        }
        let scores = enum_map! { color =>
            PlayerScore {
                longest_line: best_line[color],
                longest_loop: best_loop[color],
                score: best_line[color].max(best_loop[color] * 2),
            }
        };
        (scores, best)
    }

    #[test]
    fn scores_match_path_by_path_scoring() {
        let mut loops = 0;
        for_each_seeded_state(40, |game| {
            let board = game.board();
            let (scores, paths) = score_path_by_path(board);
            assert_eq!(board.scores(), scores, "{board:?}");
            assert_eq!(board.scoring_paths(), paths, "{board:?}");
            loops += scores.values().filter(|s| s.longest_loop > 0).count();
        });
        // The games make loops, not only lines.
        assert!(loops > 0);
    }

    #[test]
    fn lines_cover_every_path_and_lead_into_empty_positions() {
        for_each_seeded_state(20, |game| {
            let board = game.board();
            let lines = board.lines();
            let scores = board.scores();
            for color in enum_iterator::all::<Color>() {
                let of_color = || lines.iter().filter(|l| l.color == color);
                let longest = |is_loop| {
                    of_color()
                        .filter(|l| l.is_loop == is_loop)
                        .map(|l| l.tiles)
                        .max()
                        .unwrap_or(0)
                };
                assert_eq!(longest(false), scores[color].longest_line);
                assert_eq!(longest(true), scores[color].longest_loop);
                // Each tile showing the color is on exactly one of its paths.
                let tiles: usize = of_color().map(|l| l.tiles).sum();
                let showing = board
                    .iter()
                    .filter(|m| m.tile.colors().contains(&color))
                    .count();
                assert_eq!(tiles, showing);
            }
            for line in &lines {
                if line.is_loop {
                    assert_eq!(line.ends, [None, None]);
                    continue;
                }
                for end in line.ends {
                    let end = end.expect("lines on a legal board end at empty positions");
                    assert!(board.is_available(&end.position));
                    let last = board.get_move(&end.position.neighbor(end.toward)).unwrap();
                    assert_eq!(last.color(end.toward.opposite()), line.color);
                    assert_eq!(
                        board.edges_toward(end.position).get(end.toward),
                        Some(line.color)
                    );
                }
            }
        });
    }

    #[test]
    fn scores_after_a_move_match_scoring_the_board_with_it() {
        let (mut checked, mut closed) = (0, 0);
        for_each_seeded_state(12, |game| {
            let board = game.board();
            let scores = board.scores();
            for m in game.legal_moves() {
                let mut placed = board.clone();
                placed.insert_move(*m).unwrap();
                let expected = placed.scores();
                assert_eq!(
                    board.scores_after(&scores, m),
                    expected,
                    "{m:?} on {board:?}"
                );
                let indexed = board.indexed();
                assert_eq!(indexed.scores_after(&scores, m), expected);
                for color in enum_iterator::all::<Color>() {
                    assert_eq!(
                        board.score_after(scores[color].score, m, color),
                        expected[color].score,
                        "{color:?} {m:?} on {board:?}"
                    );
                    assert_eq!(
                        indexed.score_after(scores[color].score, m, color),
                        expected[color].score,
                    );
                }
                checked += 1;
                closed += usize::from(
                    expected
                        .values()
                        .zip(scores.values())
                        .any(|(after, before)| after.longest_loop > before.longest_loop),
                );
            }
        });
        // Moves that close a loop are checked too.
        assert!(
            checked > 10_000 && closed > 0,
            "{checked} checked, {closed} close a loop"
        );
    }

    #[test]
    fn test_color_link() {
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
        let right = Position::CENTER.neighbor(Direction::Right);

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
        let left = Position::CENTER.neighbor(Direction::Left);
        let right = Position::CENTER.neighbor(Direction::Right);

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
            Position::CENTER.neighbor(Direction::Left),     // (-1,0)
            Position::CENTER.neighbor(Direction::TopLeft),  // (0,1)
            Position::CENTER.neighbor(Direction::TopRight), // (1,1)
            Position::CENTER.neighbor(Direction::Right),    // (1,0)
            Position::CENTER.neighbor(Direction::BottomRight), // (0,-1)
            Position::CENTER.neighbor(Direction::BottomLeft), // (-1,-1)
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
}
