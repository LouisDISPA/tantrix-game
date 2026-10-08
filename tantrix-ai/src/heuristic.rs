use std::borrow::Cow;

use enum_map::EnumMap;
use tantrix_game::{AiView, Color, Move, TantrixAi};

use crate::eval::{Evaluator, Weights};
use crate::tile_set::TileSet;

/// Plays the legal move that leaves the best position by the line
/// evaluation of [`crate::eval`], one move ahead.
#[derive(Clone)]
pub struct HeuristicAi {
    name: Cow<'static, str>,
    weights: Weights,
}

impl HeuristicAi {
    /// Evaluates positions with `weights`.
    pub fn new(name: impl Into<Cow<'static, str>>, weights: Weights) -> Self {
        Self {
            name: name.into(),
            weights,
        }
    }

    /// Grows its own lines and keeps their ends open, and spoils the
    /// opponents' as much.
    pub fn strategist() -> Self {
        Self::new("Strategist", Weights::BALANCED)
    }

    /// Grows its own lines and keeps their ends open; ignores the opponents.
    pub fn builder() -> Self {
        Self::new("Builder", Weights::SELFISH)
    }
}

impl TantrixAi for HeuristicAi {
    fn name(&self) -> &str {
        &self.name
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn choose_move(&self, view: &AiView) -> Move {
        ranked_moves(view, &self.weights, 1)[0].0
    }
}

/// The `count` best of `view.legal_moves` by their [`Evaluator::value_after`]
/// for the current player, best first (at least one).
pub(crate) fn ranked_moves(view: &AiView, weights: &Weights, count: usize) -> Vec<(Move, f32)> {
    let evaluator = Evaluator {
        weights,
        playing: view.playing_colors,
    };
    let pool = TileSet::off(view.board);
    let hands = hand_sets(view);
    let mut ranked: Vec<(Move, f32)> = view
        .legal_moves
        .iter()
        .map(|m| {
            let value = evaluator.value_after(view.board, m, pool, &hands, view.current_player);
            (*m, value)
        })
        .collect();
    // Stable: equal moves keep their legal-move order.
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    ranked.truncate(count.max(1));
    ranked
}

pub(crate) fn hand_sets(view: &AiView) -> EnumMap<Color, TileSet> {
    EnumMap::from_fn(|color| TileSet::of(&view.hands[color]))
}
