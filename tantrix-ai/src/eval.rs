//! Position evaluation for the planning AIs: what each color's lines are
//! worth, counting what they can still grow into as well as their length.
//!
//! A color scores its longest line (1 point per tile) or loop (2 per tile),
//! so it is worth its best line: the line's tiles, plus a bonus for each
//! end a remaining tile can still extend (by the golden rule, any tile
//! placed at a line's end extends it, whoever plays it). An end a forced
//! space opens on is likely filled soon; one a tile of the owner's own hand
//! fits, the owner can extend on their next turn. Two ends leading into one
//! space are worth a share of the line or loop a single tile would make.

use enum_map::EnumMap;
use tantrix_game::{Board, Color, Line, Move, Position};

use crate::tile_set::TileSet;

/// What the parts of a line's value weigh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    /// Per end of a line that a remaining tile can extend, at the start of
    /// the game; it shrinks with the tiles left to place.
    pub open_end: f32,
    /// More per open end on a forced space.
    pub forced_end: f32,
    /// More per open end that a tile of the owner's hand fits.
    pub ready_end: f32,
    /// The share of a loop's points for a line whose ends lead into one
    /// space that a remaining tile can close.
    pub closable_loop: f32,
    /// The share of their joined length for two lines whose ends lead into
    /// one space that a remaining tile can join them through.
    pub joinable: f32,
    /// How much the best opponent's value counts against the player's.
    pub opponent: f32,
}

impl Weights {
    /// Tuned in the tournament (see `doc/ai.md`): ends on forced spaces
    /// weigh most, tiles in hand least.
    pub const BALANCED: Weights = Weights {
        open_end: 3.0,
        forced_end: 3.0,
        ready_end: 0.5,
        closable_loop: 0.25,
        joinable: 1.0,
        opponent: 1.0,
    };

    /// [`Self::BALANCED`] blind to the opponents.
    pub const SELFISH: Weights = Weights {
        opponent: 0.0,
        ..Self::BALANCED
    };
}

/// What [`Evaluator::value`] needs to know besides the board.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Evaluator<'a> {
    pub weights: &'a Weights,
    /// The colors in the game.
    pub playing: &'a [Color],
}

impl Evaluator<'_> {
    /// How good `board` is for `player`: their value less the best
    /// opponent's. `pool` holds the tiles still to place, `hands` each
    /// color's hand.
    pub(crate) fn value(
        &self,
        board: &Board,
        pool: TileSet,
        hands: &EnumMap<Color, TileSet>,
        player: Color,
    ) -> f32 {
        let lines = board.lines();
        let open_end = self.weights.open_end * pool.len() as f32 / 56.0;
        let mut own = 0.0;
        let mut best_opponent: f32 = 0.0;
        for &color in self.playing {
            let value = self.color_value(board, &lines, color, pool, hands[color], open_end);
            if color == player {
                own = value;
            } else {
                best_opponent = best_opponent.max(value);
            }
        }
        own - self.weights.opponent * best_opponent
    }

    /// [`Self::value`] of `board` once `m` is placed from `player`'s hand.
    pub(crate) fn value_after(
        &self,
        board: &Board,
        m: &Move,
        pool: TileSet,
        hands: &EnumMap<Color, TileSet>,
        player: Color,
    ) -> f32 {
        let mut after = board.clone();
        after
            .insert_move(*m)
            .expect("a legal move goes on an empty position");
        let mut hands = *hands;
        hands[player] = hands[player].without(m.tile);
        self.value(&after, pool.without(m.tile), &hands, player)
    }

    /// The value of `color`'s best line ([`self`](crate::eval)).
    fn color_value(
        &self,
        board: &Board,
        lines: &[Line],
        color: Color,
        pool: TileSet,
        hand: TileSet,
        open_end: f32,
    ) -> f32 {
        let w = self.weights;
        let mut best: f32 = 0.0;
        // The open ends: where they lead, and whose line they end.
        let mut ends: Vec<(Position, usize)> = Vec::new();
        for (index, line) in lines.iter().enumerate().filter(|(_, l)| l.color == color) {
            let tiles = line.tiles as f32;
            if line.is_loop {
                best = best.max(2.0 * tiles);
                continue;
            }
            let mut value = tiles;
            for end in line.ends.iter().flatten() {
                let edges = board.edges_toward(end.position);
                if !pool.any_fits(edges) {
                    continue;
                }
                value += open_end;
                if edges.count() >= 3 {
                    value += w.forced_end;
                }
                if hand.any_fits(edges) {
                    value += w.ready_end;
                }
                ends.push((end.position, index));
            }
            best = best.max(value);
        }

        // Ends that lead into the same open space.
        ends.sort_unstable();
        for pair in ends.windows(2) {
            let ((at_a, a), (at_b, b)) = (pair[0], pair[1]);
            if at_a != at_b {
                continue;
            }
            let joined = if a == b {
                w.closable_loop * 2.0 * (lines[a].tiles + 1) as f32
            } else {
                w.joinable * (lines[a].tiles + lines[b].tiles + 1) as f32
            };
            best = best.max(joined);
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use enum_map::EnumMap;
    use tantrix_game::{Color, Game, Player};

    use super::{Evaluator, Weights};
    use crate::tile_set::TileSet;

    #[test]
    fn without_bonuses_a_position_is_worth_its_score_difference() {
        let plain = Weights {
            open_end: 0.0,
            forced_end: 0.0,
            ready_end: 0.0,
            closable_loop: 0.0,
            joinable: 0.0,
            opponent: 1.0,
        };
        let mut game = Game::new_seeded(
            Player::Someone,
            Player::Someone,
            Player::None,
            Player::None,
            3,
        )
        .unwrap();
        for _ in 0..30 {
            if game.skip_stuck_players() || game.is_game_over() {
                break;
            }
            let m = game.legal_moves()[0];
            game.play_move(m.placement()).unwrap();

            let evaluator = Evaluator {
                weights: &plain,
                playing: game.playing_colors(),
            };
            let pool = TileSet::off(game.board());
            let hands = EnumMap::from_fn(|c| TileSet::of(game.hand(c)));
            let scores = game.scores();
            let value = evaluator.value(game.board(), pool, &hands, Color::Red);
            let expected = scores[Color::Red].score as f32 - scores[Color::Blue].score as f32;
            assert_eq!(value, expected);
        }
    }
}
