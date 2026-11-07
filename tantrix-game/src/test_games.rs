//! Seeded games for tests that compare two ways of computing the same
//! thing, or check an invariant, over many game states, and the rules
//! worked out plainly from the tiles around a move, to check moves against.

use std::collections::HashMap;

use rand::rngs::Xoshiro256PlusPlus;
use rand::{Rng, SeedableRng};

use crate::{Board, Color, Game, GameStatus, Move, Player, Position};

/// Plays `games` seeded games of two to four players, each move picked at
/// random (seeded) among the legal ones, and calls `visit` on every state
/// on the way, the deal and the end included.
pub(crate) fn for_each_seeded_state(games: u64, visit: impl FnMut(&Game)) {
    for_each_seeded_state_after(&Board::default(), games, visit);
}

/// [`for_each_seeded_state`], with `opening` already on the board when each
/// game starts ([`Game::with_opening`]): seeded play on from a hand-built
/// position that random play would hardly reach.
pub(crate) fn for_each_seeded_state_after(
    opening: &Board,
    games: u64,
    mut visit: impl FnMut(&Game),
) {
    for seed in 0..games {
        let player = |color_index: u64| {
            if color_index < 2 + seed % 3 {
                Player::Someone
            } else {
                Player::None
            }
        };
        let mut game = Game::new_seeded(player(0), player(1), player(2), player(3), seed).unwrap();
        if !opening.is_empty() {
            game = game.with_opening(opening.clone());
        }
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        loop {
            visit(&game);
            if game.status() != GameStatus::Playing {
                break;
            }
            if game.legal_moves().is_empty() {
                game.skip_stuck_players();
                continue;
            }
            let moves = game.legal_moves();
            let m = moves[(rng.next_u64() % moves.len() as u64) as usize];
            game.play_move(m.placement()).unwrap();
        }
    }
}

/// The board's tiles by position.
pub(crate) fn tiles_of(board: &Board) -> HashMap<Position, Move> {
    board.iter().map(|m| (m.position, *m)).collect()
}

/// A rule that a move breaks ([`broken_rule`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrokenRule {
    /// A tile is there already.
    Occupied,
    /// The board has tiles, and none is next to the move.
    Detached,
    /// An edge shows another color than the tile it touches.
    GoldenRule,
    /// An empty neighbor would have four tiles or more around it: the move
    /// surrounds a forced space with a fourth tile.
    FourthNeighbor,
    /// An empty neighbor would have three tiles around it, all showing it
    /// the same color: a forced space no tile can fill.
    SameColorTriple,
}

impl BrokenRule {
    /// Whether the rule applies only while the bag has tiles.
    pub(crate) fn is_restriction(self) -> bool {
        matches!(self, Self::FourthNeighbor | Self::SameColorTriple)
    }
}

/// The rule that playing `m` among `tiles` breaks while the bag has
/// tiles, if any, worked out plainly from the tiles around it rather
/// than with the move generator's helpers. The restriction rules come
/// last: a move that breaks one keeps the others.
pub(crate) fn broken_rule(tiles: &HashMap<Position, Move>, m: &Move) -> Option<BrokenRule> {
    if tiles.contains_key(&m.position) {
        return Some(BrokenRule::Occupied);
    }
    if !tiles.is_empty() && !m.position.around().any(|(p, _)| tiles.contains_key(&p)) {
        return Some(BrokenRule::Detached);
    }
    for (p, dir) in m.position.around() {
        if tiles
            .get(&p)
            .is_some_and(|t| t.color(dir.opposite()) != m.color(dir))
        {
            return Some(BrokenRule::GoldenRule);
        }
    }
    // Only the empty positions around the move gain a neighbor.
    for (space, _) in m.position.around() {
        if tiles.contains_key(&space) {
            continue;
        }
        let colors: Vec<Color> = space
            .around()
            .filter_map(|(p, dir)| {
                let tile = if p == m.position {
                    Some(m)
                } else {
                    tiles.get(&p)
                };
                tile.map(|t| t.color(dir.opposite()))
            })
            .collect();
        if let Some(rule) = space_rule(&colors) {
            return Some(rule);
        }
    }
    None
}

/// The restriction rule an empty space breaks with tiles showing it
/// `colors`, if any: four tiles or more, or three of one color.
pub(crate) fn space_rule(colors: &[Color]) -> Option<BrokenRule> {
    match colors {
        [_, _, _, _, ..] => Some(BrokenRule::FourthNeighbor),
        [a, b, c] if a == b && b == c => Some(BrokenRule::SameColorTriple),
        _ => None,
    }
}
