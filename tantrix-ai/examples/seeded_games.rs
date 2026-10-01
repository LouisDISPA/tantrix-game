//! Times seeded games between every pairing of [`all_ais`], dealt and
//! seeded like the tournament's (the first AI plays Red, the second Blue):
//!
//! `cargo run --release -p tantrix-ai --example seeded_games [GAMES_PER_PAIR]`
//!
//! Then times the rules and the scoring alone on the recorded games. The
//! checksum covers every game's history and final scores: a change that must
//! not alter play leaves it unchanged.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::hint::black_box;
use std::time::{Duration, Instant};

use rand::rngs::Xoshiro256PlusPlus;
use rand::{Rng, SeedableRng};
use tantrix_ai::all_ais;
use tantrix_game::{Board, Color, EnumMap, Game, Player, TurnEvent};

struct Recorded {
    seed: u64,
    history: Vec<TurnEvent>,
}

fn main() {
    let games_per_pair: usize = std::env::args()
        .nth(1)
        .map(|arg| arg.parse().expect("GAMES_PER_PAIR must be a number"))
        .unwrap_or(20);
    let ais = all_ais();
    let mut seeds = Xoshiro256PlusPlus::seed_from_u64(0);
    let mut checksum = DefaultHasher::new();
    let mut recorded = Vec::new();
    let mut total = Duration::ZERO;

    println!("{games_per_pair} games per pairing");
    for i in 0..ais.len() {
        for j in (i + 1)..ais.len() {
            let start = Instant::now();
            for _ in 0..games_per_pair {
                let seed = seeds.next_u64();
                let mut ai_seeds = Xoshiro256PlusPlus::seed_from_u64(seed);
                let (seed_a, seed_b) = (ai_seeds.next_u64(), ai_seeds.next_u64());
                let mut game = Game::new_seeded(
                    Player::Ai(ais[i].with_seed(seed_a)),
                    Player::Ai(ais[j].with_seed(seed_b)),
                    Player::None,
                    Player::None,
                    seed,
                )
                .unwrap();
                game.advance();
                format!("{:?}", game.history()).hash(&mut checksum);
                let scores = game.scores();
                (scores[Color::Red].score, scores[Color::Blue].score).hash(&mut checksum);
                recorded.push(Recorded {
                    seed,
                    history: game.history().to_vec(),
                });
            }
            let elapsed = start.elapsed();
            total += elapsed;
            println!(
                "{:>10} vs {:<10} {:>9.2} ms",
                ais[i].name(),
                ais[j].name(),
                elapsed.as_secs_f64() * 1000.0
            );
        }
    }
    println!(
        "all games             {:>9.2} ms",
        total.as_secs_f64() * 1000.0
    );

    // The rules alone: deal and play every recorded game again.
    let players = EnumMap::from_fn(|c| match c {
        Color::Red | Color::Blue => Player::Someone,
        _ => Player::None,
    });
    let start = Instant::now();
    for game in &recorded {
        let replayed = Game::replay(game.seed, players.clone(), &game.history).unwrap();
        black_box(replayed.legal_moves().len());
    }
    println!(
        "replay (rules)        {:>9.2} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );

    // Scoring alone, on every board of every recorded game.
    let mut boards: Vec<Board> = Vec::new();
    for game in &recorded {
        let mut replayed = Game::replay(game.seed, players.clone(), &[]).unwrap();
        for event in &game.history {
            replayed.replay_event(event).unwrap();
            if matches!(event, TurnEvent::Played { .. }) {
                boards.push(replayed.board().clone());
            }
        }
    }
    let start = Instant::now();
    for board in &boards {
        black_box(board.scores());
        black_box(board.scoring_paths());
    }
    println!(
        "scoring ({} boards) {:>9.2} ms",
        boards.len(),
        start.elapsed().as_secs_f64() * 1000.0
    );

    println!("checksum {:016x}", checksum.finish());
}
