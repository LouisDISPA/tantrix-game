//! Scoring: each color's longest line (1 point per tile) or loop (2 points
//! per tile) on the board.

use std::collections::BTreeSet;

use arrayvec::ArrayVec;
use enum_map::{enum_map, EnumMap};

use crate::{Board, Color, Direction, Move, Position};

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

impl Board {
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

        for m in self.iter() {
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

        for m in self.iter() {
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
}

#[cfg(test)]
mod tests {
    use super::color_link;
    use crate::{Board, Color, Direction, Move, Position, Rotation, Tile};

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
