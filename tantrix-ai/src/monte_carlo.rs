use enum_map::EnumMap;
use rand::rngs::Xoshiro256PlusPlus;
use rand::{Rng, RngExt, SeedableRng};
use tantrix_game::{AiView, Color, Game, Move, PlayerScore, TantrixAi};

use crate::eval::Weights;
use crate::heuristic::ranked_moves;
use crate::rng::AiRng;

/// Plays the best few moves by the line evaluation of [`crate::eval`] out
/// to the end of the game, many times, and keeps the one that does best on
/// average. Each playout deals the unseen tiles into the bag in a random
/// order ([`AiView::determinize`]); every move is played out on the same
/// deals, so that luck of the draw weighs on all of them alike. Playouts
/// are greedy: each player grows their longest line, ties broken at
/// random.
///
/// It spends `width × playouts` playouts on a move. Halving (the default),
/// it spends them over rounds, keeping the better half of the moves each
/// round, so that the best moves get the most; flat, each of the `width`
/// moves gets `playouts`. Tuned in the tournament: halving beats flat at the
/// same cost, and strength grows with the playouts spent.
#[derive(Clone)]
pub struct MonteCarloAi {
    weights: Weights,
    /// How many of the best moves it plays out.
    width: usize,
    /// How many playouts per move, on average.
    playouts: usize,
    halving: bool,
    rng: AiRng,
}

impl MonteCarloAi {
    pub const DEFAULT_WIDTH: usize = 16;
    pub const DEFAULT_PLAYOUTS: usize = 16;

    /// Plays out the `width` best moves by `weights`, `playouts` times each
    /// on average, by successive halving.
    pub fn new(weights: Weights, width: usize, playouts: usize) -> Self {
        Self {
            weights,
            width,
            playouts,
            halving: true,
            rng: AiRng::default(),
        }
    }

    /// Whether to spend the playouts by successive halving, or flat.
    pub fn halving(self, halving: bool) -> Self {
        Self { halving, ..self }
    }
}

impl Default for MonteCarloAi {
    fn default() -> Self {
        Self::new(
            Weights::BALANCED,
            Self::DEFAULT_WIDTH,
            Self::DEFAULT_PLAYOUTS,
        )
    }
}

/// What a game that ended on `scores` is worth to `player`: their winning
/// margin, and more for a win.
fn outcome(scores: &EnumMap<Color, PlayerScore>, playing: &[Color], player: Color) -> f32 {
    const WIN: f32 = 5.0;
    let own = scores[player].score as f32;
    let best_opponent = playing
        .iter()
        .filter(|&&c| c != player)
        .map(|&c| scores[c].score as f32)
        .fold(0.0, f32::max);
    let margin = own - best_opponent;
    margin + WIN * margin.signum()
}

/// Play `game`, whose board scores `scores`, to its end, each move
/// [`greedy_move`]; returns the final scores.
fn play_out<R: Rng>(
    game: &mut Game,
    mut scores: EnumMap<Color, PlayerScore>,
    rng: &mut R,
) -> EnumMap<Color, PlayerScore> {
    while !game.skip_stuck_players() && !game.is_game_over() {
        let (m, after) = greedy_move(game, &scores, rng);
        scores = after;
        game.play_move(m.placement())
            .expect("the playout plays legal moves");
    }
    scores
}

/// The legal move that most raises the current player's score, given the
/// board's `scores`, ties broken at random; and the scores after it.
fn greedy_move<R: Rng>(
    game: &Game,
    scores: &EnumMap<Color, PlayerScore>,
    rng: &mut R,
) -> (Move, EnumMap<Color, PlayerScore>) {
    let board = game.board().indexed();
    let player = game.current_player();
    let now = scores[player].score;
    let mut best = (0, 0, game.legal_moves()[0]); // score, ties so far, move
    for m in game.legal_moves() {
        let score = board.score_after(now, m, player);
        if score > best.0 || best.1 == 0 {
            best = (score, 1, *m);
        } else if score == best.0 {
            best.1 += 1;
            if rng.random_range(0..best.1) == 0 {
                best.2 = *m;
            }
        }
    }
    (best.2, board.scores_after(scores, &best.2))
}

impl TantrixAi for MonteCarloAi {
    fn name(&self) -> &str {
        "MonteCarlo"
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn with_seed(&self, seed: u64) -> Box<dyn TantrixAi> {
        Box::new(Self {
            rng: AiRng::seeded(seed),
            ..self.clone()
        })
    }

    fn choose_move(&self, view: &AiView) -> Move {
        let candidates = ranked_moves(view, &self.weights, self.width);
        if candidates.len() == 1 {
            return candidates[0].0;
        }
        let mut rng = self.rng.fork();
        let scores = view.board.scores();
        let board = view.board.indexed();
        let mut totals = vec![0.0; candidates.len()];
        let mut counts = vec![0_usize; candidates.len()];
        let mean = |i: usize, totals: &[f32], counts: &[usize]| totals[i] / counts[i].max(1) as f32;

        // Candidates still in, best first.
        let mut alive: Vec<usize> = (0..candidates.len()).collect();
        let budget = candidates.len() * self.playouts;
        let rounds = if self.halving {
            candidates.len().next_power_of_two().trailing_zeros().max(1) as usize
        } else {
            1
        };
        for round in 0..rounds {
            let playouts = (budget / rounds / alive.len()).max(1);
            for _ in 0..playouts {
                let deal = view.determinize(&mut rng);
                let playout_seed = rng.next_u64();
                for &i in &alive {
                    let m = &candidates[i].0;
                    let mut game = deal.clone();
                    let after = board.scores_after(&scores, m);
                    game.play_move(m.placement())
                        .expect("candidates are legal moves");
                    let mut playout_rng = Xoshiro256PlusPlus::seed_from_u64(playout_seed);
                    let end = play_out(&mut game, after, &mut playout_rng);
                    totals[i] += outcome(&end, view.playing_colors, view.current_player);
                    counts[i] += 1;
                }
            }
            if round + 1 < rounds {
                // Stable: of equal moves, the evaluation's better one stays.
                alive.sort_by(|&a, &b| {
                    mean(b, &totals, &counts).total_cmp(&mean(a, &totals, &counts))
                });
                alive.truncate(alive.len().div_ceil(2));
                alive.sort_unstable();
            }
        }
        // The first of the best: candidates come best first.
        let best = alive.iter().copied().fold(alive[0], |best, i| {
            if mean(i, &totals, &counts) > mean(best, &totals, &counts) {
                i
            } else {
                best
            }
        });
        candidates[best].0
    }
}
