//! The round robin itself: which AIs play each game and on which deal,
//! each game's result, and the standings.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use rand::rngs::Xoshiro256PlusPlus;
use rand::{Rng, SeedableRng};
use tantrix_game::{Color, Game, Player, TantrixAi};

/// How many of each AI's latest scores the standings keep.
const RECENT_SCORES: usize = 60;

/// Every pairing of `ais` plays `games_per_pair` games in a row, the first
/// AI of the pairing with each later one, then the second… Each game's
/// seed is drawn in order from the tournament seed before any game runs,
/// and deals the game and seeds both AIs, so a tournament gives the same
/// results however its games are scheduled.
pub struct Tournament {
    pub seed: u64,
    pub ais: Vec<Box<dyn TantrixAi>>,
    pub games_per_pair: usize,
    /// Every deal is played twice in a row, the second time with the AIs'
    /// seats swapped, so neither gets the better hands or turn order.
    pub mirrored: bool,
    /// One per game, in game order.
    seeds: Vec<u64>,
}

/// Who plays a game, and its deal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// Indices among the tournament's AIs.
    pub red: usize,
    pub blue: usize,
    pub seed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameResult {
    pub red: usize,
    pub blue: usize,
    pub red_score: usize,
    pub blue_score: usize,
    /// The turn order is dealt at random: whether Red had the first turn.
    pub red_moved_first: bool,
    /// The time each AI took to choose its moves, Red's then Blue's.
    pub think: [Duration; 2],
    pub moves: [u32; 2],
}

impl GameResult {
    /// `(ai, score, opponent's score)` for both AIs.
    pub fn sides(&self) -> [(usize, usize, usize); 2] {
        [
            (self.red, self.red_score, self.blue_score),
            (self.blue, self.blue_score, self.red_score),
        ]
    }

    pub fn winner(&self) -> Option<usize> {
        match self.red_score.cmp(&self.blue_score) {
            std::cmp::Ordering::Greater => Some(self.red),
            std::cmp::Ordering::Less => Some(self.blue),
            std::cmp::Ordering::Equal => None,
        }
    }
}

impl Tournament {
    pub fn new(
        seed: u64,
        ais: Vec<Box<dyn TantrixAi>>,
        games_per_pair: usize,
        mirrored: bool,
    ) -> Self {
        let pairs = ais.len() * ais.len().saturating_sub(1) / 2;
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        let seeds = (0..pairs * games_per_pair)
            .map(|_| rng.next_u64())
            .collect();
        Self {
            seed,
            ais,
            games_per_pair,
            mirrored,
            seeds,
        }
    }

    pub fn games(&self) -> usize {
        self.seeds.len()
    }

    pub fn names(&self) -> Vec<String> {
        self.ais.iter().map(|ai| ai.name().to_string()).collect()
    }

    /// The AIs (indices) that play game `game`, in pairing order.
    pub fn pairing(&self, game: usize) -> (usize, usize) {
        let mut pair = game / self.games_per_pair;
        let count = self.ais.len();
        for a in 0..count {
            let later = count - a - 1;
            if pair < later {
                return (a, a + 1 + pair);
            }
            pair -= later;
        }
        panic!("the tournament has no game {game}")
    }

    /// Who plays game `game`, and on which deal: the pairing's first AI
    /// plays Red, but on the second game of a mirrored deal.
    pub fn plan(&self, game: usize) -> Plan {
        let (a, b) = self.pairing(game);
        let second = self.mirrored && (game % self.games_per_pair) % 2 == 1;
        let deal = if second { game - 1 } else { game };
        let (red, blue) = if second { (b, a) } else { (a, b) };
        Plan {
            red,
            blue,
            seed: self.seeds[deal],
        }
    }

    pub fn play(&self, game: usize) -> GameResult {
        play(&self.ais, self.plan(game))
    }

    /// The game to play `turn`-th: the first game of every pairing, then
    /// the second of every pairing… so that the games played so far, at any
    /// time, are spread over every pairing.
    pub fn scheduled(&self, turn: usize) -> usize {
        let pairs = self.games() / self.games_per_pair.max(1);
        (turn % pairs) * self.games_per_pair + turn / pairs
    }
}

/// Play `plan` to the end: `ais[plan.red]` as Red against `ais[plan.blue]`
/// as Blue, on the deal of `plan.seed`, which also seeds both AIs.
pub fn play(ais: &[Box<dyn TantrixAi>], plan: Plan) -> GameResult {
    let game = play_game(ais[plan.red].as_ref(), ais[plan.blue].as_ref(), plan.seed);
    let scores = game.game.scores();
    GameResult {
        red: plan.red,
        blue: plan.blue,
        red_score: scores[Color::Red].score,
        blue_score: scores[Color::Blue].score,
        red_moved_first: game.game.playing_colors()[0] == Color::Red,
        think: game.think,
        moves: game.moves,
    }
}

/// A game played to the end, with the time each AI took over it.
pub struct Played {
    pub game: Game,
    pub think: [Duration; 2],
    pub moves: [u32; 2],
}

/// Play `red` against `blue` to the end, on the deal of `seed`, which also
/// seeds both AIs.
pub fn play_game(red: &dyn TantrixAi, blue: &dyn TantrixAi, seed: u64) -> Played {
    let mut seeds = Xoshiro256PlusPlus::seed_from_u64(seed);
    let ais = [
        red.with_seed(seeds.next_u64()),
        blue.with_seed(seeds.next_u64()),
    ];
    let mut game = Game::new_seeded(
        Player::Someone,
        Player::Someone,
        Player::None,
        Player::None,
        seed,
    )
    .expect("tournament games have two players");
    let mut think = [Duration::ZERO; 2];
    let mut moves = [0; 2];
    // Like `Game::advance`, timing each choice.
    while !game.skip_stuck_players() && !game.is_game_over() {
        let seat = usize::from(game.current_player() == Color::Blue);
        let start = Instant::now();
        let m = ais[seat].choose_move(&game.ai_view());
        think[seat] += start.elapsed();
        moves[seat] += 1;
        game.play_move(m.placement())
            .expect("AIs choose among the legal moves");
    }
    Played { game, think, moves }
}

/// Wins, losses and draws.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Record {
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
}

impl Record {
    pub fn games(&self) -> u32 {
        self.wins + self.losses + self.draws
    }

    /// A win is a point, a draw half of one.
    pub fn points(&self) -> f64 {
        self.wins as f64 + self.draws as f64 / 2.0
    }

    /// The share of the points played for won, from 0 to 1.
    pub fn rate(&self) -> f64 {
        match self.games() {
            0 => 0.0,
            games => self.points() / games as f64,
        }
    }

    /// Half the width of the 95% confidence interval of [`Self::rate`].
    pub fn margin_of_error(&self) -> f64 {
        match self.games() {
            0 => 0.0,
            games => 1.96 * (self.rate() * (1.0 - self.rate()) / games as f64).sqrt(),
        }
    }

    fn add(&mut self, own: usize, opponent: usize) {
        match own.cmp(&opponent) {
            std::cmp::Ordering::Greater => self.wins += 1,
            std::cmp::Ordering::Less => self.losses += 1,
            std::cmp::Ordering::Equal => self.draws += 1,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AiStats {
    pub name: String,
    pub record: Record,
    pub total_score: u64,
    pub total_margin: i64,
    pub best_score: usize,
    pub worst_score: Option<usize>,
    pub think: Duration,
    pub moves: u64,
    pub recent_scores: VecDeque<u64>,
}

impl AiStats {
    pub fn avg_score(&self) -> f64 {
        self.per_game(self.total_score as f64)
    }

    pub fn avg_margin(&self) -> f64 {
        self.per_game(self.total_margin as f64)
    }

    fn per_game(&self, total: f64) -> f64 {
        match self.record.games() {
            0 => 0.0,
            games => total / games as f64,
        }
    }

    /// The average time it took to choose a move, in milliseconds.
    pub fn ms_per_move(&self) -> f64 {
        match self.moves {
            0 => 0.0,
            moves => self.think.as_secs_f64() * 1000.0 / moves as f64,
        }
    }
}

/// The results so far, per AI (in the tournament's order) and per pairing.
#[derive(Debug, Clone)]
pub struct Standings {
    pub stats: Vec<AiStats>,
    /// `head_to_head[a][b]`: `a`'s record against `b`.
    pub head_to_head: Vec<Vec<Record>>,
    pub games: u32,
    /// How many games the first player to move won, lost and drew.
    pub first_mover: Record,
}

impl Standings {
    pub fn new(names: &[String]) -> Self {
        Self {
            stats: names
                .iter()
                .map(|name| AiStats {
                    name: name.clone(),
                    ..Default::default()
                })
                .collect(),
            head_to_head: vec![vec![Record::default(); names.len()]; names.len()],
            games: 0,
            first_mover: Record::default(),
        }
    }

    pub fn record(&mut self, result: &GameResult) {
        self.games += 1;
        for (seat, (ai, own, opponent)) in result.sides().into_iter().enumerate() {
            let stat = &mut self.stats[ai];
            stat.record.add(own, opponent);
            stat.total_score += own as u64;
            stat.total_margin += own as i64 - opponent as i64;
            stat.best_score = stat.best_score.max(own);
            stat.worst_score = Some(stat.worst_score.map_or(own, |worst| worst.min(own)));
            stat.think += result.think[seat];
            stat.moves += result.moves[seat] as u64;
            stat.recent_scores.push_back(own as u64);
            if stat.recent_scores.len() > RECENT_SCORES {
                stat.recent_scores.pop_front();
            }
        }
        self.head_to_head[result.red][result.blue].add(result.red_score, result.blue_score);
        self.head_to_head[result.blue][result.red].add(result.blue_score, result.red_score);
        let (first, second) = if result.red_moved_first {
            (result.red_score, result.blue_score)
        } else {
            (result.blue_score, result.red_score)
        };
        self.first_mover.add(first, second);
    }

    /// Elo ratings fitted to every head-to-head record (a Bradley–Terry
    /// model, draws counting half a win each way), averaging 1500. Each
    /// pairing counts one draw more than it played, so an AI that never
    /// wins still gets a finite rating.
    pub fn elo(&self) -> Vec<f64> {
        let n = self.stats.len();
        let played = |i: usize, j: usize| self.head_to_head[i][j].games() as f64;
        let wins: Vec<f64> = (0..n)
            .map(|i| {
                (0..n)
                    .filter(|&j| j != i && played(i, j) > 0.0)
                    .map(|j| self.head_to_head[i][j].points() + 0.5)
                    .sum()
            })
            .collect();
        let mut strength = vec![1.0_f64; n];
        for _ in 0..10_000 {
            let mut next: Vec<f64> = (0..n)
                .map(|i| {
                    let expected: f64 = (0..n)
                        .filter(|&j| j != i && played(i, j) > 0.0)
                        .map(|j| (played(i, j) + 1.0) / (strength[i] + strength[j]))
                        .sum();
                    if expected > 0.0 {
                        wins[i] / expected
                    } else {
                        strength[i]
                    }
                })
                .collect();
            // Keep the geometric mean at 1: the average rating at 1500.
            let mean = next.iter().map(|s| s.ln()).sum::<f64>() / n.max(1) as f64;
            next.iter_mut().for_each(|s| *s /= mean.exp());
            let change = next
                .iter()
                .zip(&strength)
                .map(|(a, b)| (a / b).ln().abs())
                .fold(0.0, f64::max);
            strength = next;
            if change < 1e-10 {
                break;
            }
        }
        strength
            .iter()
            .map(|s| 1500.0 + 400.0 * s.log10())
            .collect()
    }

    /// The AIs (indices), best first: by Elo, then by points.
    pub fn ranking(&self) -> Vec<usize> {
        let elo = self.elo();
        let mut order: Vec<usize> = (0..self.stats.len()).collect();
        order.sort_by(|&a, &b| {
            elo[b].total_cmp(&elo[a]).then(
                self.stats[b]
                    .record
                    .rate()
                    .total_cmp(&self.stats[a].record.rate()),
            )
        });
        order
    }
}

#[cfg(test)]
mod tests {
    use rayon::prelude::*;
    use tantrix_ai::{FirstMoveAi, GreedyAi, RandomAi, SmartAi};
    use tantrix_game::{Color, TantrixAi};

    use super::*;

    fn quick_ais() -> Vec<Box<dyn TantrixAi>> {
        vec![
            Box::new(FirstMoveAi),
            Box::new(RandomAi::new()),
            Box::new(GreedyAi),
            Box::new(SmartAi),
        ]
    }

    #[test]
    fn a_seed_plays_the_same_game_again() {
        let ais: Vec<Box<dyn TantrixAi>> = vec![Box::new(RandomAi::new()), Box::new(SmartAi)];
        for seed in 0..3 {
            let plan = Plan {
                red: 0,
                blue: 1,
                seed,
            };
            let (first, again) = (play(&ais, plan), play(&ais, plan));
            assert_eq!(
                (first.red_score, first.blue_score, first.moves),
                (again.red_score, again.blue_score, again.moves)
            );
        }
    }

    #[test]
    fn each_ai_scores_the_color_it_plays() {
        let ais: Vec<Box<dyn TantrixAi>> = vec![Box::new(FirstMoveAi), Box::new(GreedyAi)];
        let mut blue_first_with_other_scores = 0;
        for seed in 0..10 {
            let played = play_game(ais[0].as_ref(), ais[1].as_ref(), seed);
            let result = play(
                &ais,
                Plan {
                    red: 0,
                    blue: 1,
                    seed,
                },
            );

            // FirstMove is Red and Greedy Blue, whoever the deal lets move first.
            let scores = played.game.scores();
            assert_eq!(result.red_score, scores[Color::Red].score, "seed {seed}");
            assert_eq!(result.blue_score, scores[Color::Blue].score, "seed {seed}");
            let red_first = played.game.playing_colors()[0] == Color::Red;
            assert_eq!(result.red_moved_first, red_first);
            if !red_first && result.red_score != result.blue_score {
                blue_first_with_other_scores += 1;
            }
            assert!(result.moves[0] > 0 && result.moves[1] > 0);
        }
        // Scoring by turn order instead of color would fail these games.
        assert!(blue_first_with_other_scores > 0);
    }

    #[test]
    fn a_game_played_with_timing_is_the_game_the_ais_advance_to() {
        let (red, blue) = (SmartAi, RandomAi::new());
        for seed in 0..5 {
            let played = play_game(&red, &blue, seed);
            let mut seeds = Xoshiro256PlusPlus::seed_from_u64(seed);
            let mut game = Game::new_seeded(
                Player::Ai(red.with_seed(seeds.next_u64())),
                Player::Ai(blue.with_seed(seeds.next_u64())),
                Player::None,
                Player::None,
                seed,
            )
            .unwrap();
            game.advance();
            assert_eq!(played.game.history(), game.history(), "seed {seed}");
        }
    }

    #[test]
    fn every_pairing_plays_its_games_in_a_row() {
        let tournament = Tournament::new(0, quick_ais(), 3, false);
        let count = tournament.ais.len();
        let mut expected = Vec::new();
        for a in 0..count {
            for b in (a + 1)..count {
                expected.extend(std::iter::repeat_n((a, b), 3));
            }
        }
        assert_eq!(tournament.games(), expected.len());
        let pairings: Vec<_> = (0..tournament.games())
            .map(|game| tournament.pairing(game))
            .collect();
        assert_eq!(pairings, expected);
    }

    #[test]
    fn games_are_scheduled_round_by_round_over_every_pairing() {
        let tournament = Tournament::new(0, quick_ais(), 4, true);
        let order: Vec<usize> = (0..tournament.games())
            .map(|turn| tournament.scheduled(turn))
            .collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..tournament.games()).collect::<Vec<_>>());
        // The first round plays every pairing once.
        let pairs = tournament.games() / 4;
        let mut first_round: Vec<_> = order[..pairs]
            .iter()
            .map(|&game| tournament.pairing(game))
            .collect();
        first_round.dedup();
        assert_eq!(first_round.len(), pairs);
    }

    #[test]
    fn a_mirrored_deal_is_played_again_with_the_seats_swapped() {
        let tournament = Tournament::new(0, quick_ais(), 5, true);
        for game in 0..tournament.games() {
            let (a, b) = tournament.pairing(game);
            let plan = tournament.plan(game);
            match game % 5 {
                1 | 3 => {
                    let first = tournament.plan(game - 1);
                    assert_eq!((plan.red, plan.blue), (b, a));
                    assert_eq!(plan.seed, first.seed);
                }
                // The last game of an odd number has no mirror.
                _ => {
                    assert_eq!((plan.red, plan.blue), (a, b));
                    if game > 0 {
                        assert_ne!(plan.seed, tournament.plan(game - 1).seed);
                    }
                }
            }
        }
        // Unmirrored, every game has its own deal and the first AI plays Red.
        let fresh = Tournament::new(0, quick_ais(), 5, false);
        for game in 0..fresh.games() {
            assert_eq!(
                (fresh.plan(game).red, fresh.plan(game).blue),
                fresh.pairing(game)
            );
        }
    }

    #[test]
    fn a_tournament_does_not_depend_on_scheduling() {
        let tournament = Tournament::new(0, quick_ais(), 2, true);
        assert_eq!(
            tournament.seeds,
            Tournament::new(0, quick_ais(), 2, true).seeds
        );
        assert_ne!(
            tournament.seeds,
            Tournament::new(1, quick_ais(), 2, true).seeds
        );

        let score = |r: GameResult| (r.red, r.blue, r.red_score, r.blue_score, r.moves);
        let games: Vec<usize> = (0..tournament.games()).collect();
        let in_order: Vec<_> = games.iter().map(|&g| score(tournament.play(g))).collect();
        let in_parallel: Vec<_> = games
            .par_iter()
            .map(|&g| score(tournament.play(g)))
            .collect();
        assert_eq!(in_order, in_parallel);
    }

    fn result(red: usize, blue: usize, red_score: usize, blue_score: usize) -> GameResult {
        GameResult {
            red,
            blue,
            red_score,
            blue_score,
            red_moved_first: true,
            think: [Duration::from_millis(2), Duration::from_millis(6)],
            moves: [2, 3],
        }
    }

    #[test]
    fn standings_count_each_side_of_a_game() {
        let names = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let mut standings = Standings::new(&names);
        standings.record(&result(0, 1, 10, 4));
        standings.record(&result(1, 0, 7, 7));
        standings.record(&result(2, 0, 9, 3));

        let a = &standings.stats[0];
        assert_eq!(
            a.record,
            Record {
                wins: 1,
                losses: 1,
                draws: 1
            }
        );
        assert_eq!(a.total_score, 20);
        assert_eq!(a.total_margin, 6 - 6);
        assert_eq!((a.best_score, a.worst_score), (10, Some(3)));
        assert_eq!(a.moves, 2 + 3 + 3);
        assert_eq!(standings.head_to_head[0][1].points(), 1.5);
        assert_eq!(standings.head_to_head[1][0].points(), 0.5);
        assert_eq!(standings.head_to_head[0][2].points(), 0.0);
        assert_eq!(standings.first_mover.points(), 2.5);
        assert_eq!(standings.games, 3);
    }

    #[test]
    fn elo_ranks_the_stronger_ai_higher_around_1500() {
        let names = vec!["Weak".to_string(), "Mid".to_string(), "Strong".to_string()];
        let mut standings = Standings::new(&names);
        let mut play = |winner: usize, loser: usize, times: usize| {
            for _ in 0..times {
                standings.record(&result(winner, loser, 2, 1));
            }
        };
        play(1, 0, 75);
        play(0, 1, 25);
        play(2, 1, 75);
        play(1, 2, 25);
        play(2, 0, 90);
        play(0, 2, 10);

        let elo = standings.elo();
        assert!(elo[2] > elo[1] && elo[1] > elo[0], "{elo:?}");
        let mean = elo.iter().sum::<f64>() / 3.0;
        assert!((mean - 1500.0).abs() < 1.0, "{elo:?}");
        // Winning 75% is about 190 Elo.
        assert!((elo[1] - elo[0] - 190.0).abs() < 40.0, "{elo:?}");
        assert_eq!(standings.ranking(), vec![2, 1, 0]);
    }
}
