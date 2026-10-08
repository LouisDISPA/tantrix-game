use enum_map::EnumMap;
use tantrix_game::{AiView, Bag, Board, Color, Move, TantrixAi, TurnStep};

use crate::eval::{Evaluator, Weights};
use crate::heuristic::{hand_sets, ranked_moves};
use crate::tile_set::TileSet;

/// Looks two moves ahead: tries the best few moves by the line evaluation
/// of [`crate::eval`], and answers each with the best next move, by the
/// same evaluation, of whoever plays next (hands are face up). The tile it
/// draws is unknown, so it is left out.
#[derive(Clone)]
pub struct LookaheadAi {
    weights: Weights,
    /// How many of the best moves it looks ahead from.
    width: usize,
}

impl LookaheadAi {
    pub const DEFAULT_WIDTH: usize = 8;

    /// Looks ahead from the `width` best moves by `weights`.
    pub fn new(weights: Weights, width: usize) -> Self {
        Self { weights, width }
    }
}

impl Default for LookaheadAi {
    fn default() -> Self {
        Self::new(Weights::BALANCED, Self::DEFAULT_WIDTH)
    }
}

impl TantrixAi for LookaheadAi {
    fn name(&self) -> &str {
        "Lookahead"
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn choose_move(&self, view: &AiView) -> Move {
        let candidates = ranked_moves(view, &self.weights, self.width);
        if candidates.len() == 1 {
            return candidates[0].0;
        }
        let evaluator = Evaluator {
            weights: &self.weights,
            playing: view.playing_colors,
        };
        candidates
            .iter()
            .map(|(m, value)| (m, self.reply_value(view, &evaluator, m).unwrap_or(*value)))
            // The first of the best: candidates come best first.
            .fold(None, |best: Option<(&Move, f32)>, (m, value)| match best {
                Some((_, top)) if top >= value => best,
                _ => Some((m, value)),
            })
            .map(|(m, _)| *m)
            .expect("there are candidates")
    }
}

impl LookaheadAi {
    /// The value for the current player of `m` once the next move answers
    /// it, or `None` if nobody can move next.
    fn reply_value(&self, view: &AiView, evaluator: &Evaluator, m: &Move) -> Option<f32> {
        let me = view.current_player;
        let mut board = view.board.clone();
        board
            .insert_move(*m)
            .expect("a legal move goes on an empty position");
        let pool = TileSet::off(&board);
        let mut hands = view.hands.clone();
        // Played without drawing: the draw is unknown.
        hands[me].play_tile(m.tile, &mut Bag::with_count(0));
        let hand_sets = {
            let mut sets = hand_sets(view);
            sets[me] = sets[me].without(m.tile);
            sets
        };
        let bag_is_empty = view.bag_count <= 1;

        // A forced move leaves the turn's free move to play, and any move
        // may open forced spaces to fill: then the same player moves again.
        let step = if m.forced {
            view.turn_step
        } else {
            TurnStep::AfterFreeMove
        };
        let again = board.possible_moves(&hands[me], bag_is_empty, step);
        if !again.is_empty() {
            return best_value(evaluator, &board, &again, pool, &hand_sets, me);
        }

        let next = next_player(view.playing_colors, me);
        let replies = board.possible_moves(&hands[next], bag_is_empty, TurnStep::BeforeFreeMove);
        // Their gain is our loss.
        best_value(evaluator, &board, &replies, pool, &hand_sets, next).map(|value| -value)
    }
}

/// The value for `player` of the best of `moves`, by
/// [`Evaluator::value_after`]; `None` without moves.
fn best_value(
    evaluator: &Evaluator,
    board: &Board,
    moves: &[Move],
    pool: TileSet,
    hands: &EnumMap<Color, TileSet>,
    player: Color,
) -> Option<f32> {
    moves
        .iter()
        .map(|m| evaluator.value_after(board, m, pool, hands, player))
        .max_by(f32::total_cmp)
}

/// The color after `color` in turn order.
fn next_player(playing: &[Color], color: Color) -> Color {
    let index = playing.iter().position(|&c| c == color).unwrap_or(0);
    playing[(index + 1) % playing.len()]
}
