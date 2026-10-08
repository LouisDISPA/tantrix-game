pub use tantrix_game::TantrixAi;

mod random;
pub use random::RandomAi;

mod first_move;
pub use first_move::FirstMoveAi;

mod greedy;
pub use greedy::GreedyAi;

mod defensive;
pub use defensive::DefensiveAi;

mod loop_hunter;
pub use loop_hunter::LoopHunterAi;

mod smart;
pub use smart::SmartAi;

mod eval;
pub use eval::Weights;

mod heuristic;
pub use heuristic::HeuristicAi;

mod lookahead;
pub use lookahead::LookaheadAi;

mod monte_carlo;
pub use monte_carlo::MonteCarloAi;

mod rng;
mod tile_set;

/// All available AI constructors for use in tournaments.
pub fn all_ais() -> Vec<Box<dyn TantrixAi>> {
    vec![
        Box::new(FirstMoveAi),
        Box::new(RandomAi::new()),
        Box::new(GreedyAi),
        Box::new(DefensiveAi),
        Box::new(LoopHunterAi),
        Box::new(SmartAi),
        Box::new(HeuristicAi::builder()),
        Box::new(HeuristicAi::strategist()),
        Box::new(LookaheadAi::default()),
        Box::new(MonteCarloAi::default()),
    ]
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Game, Player, TantrixAi};

    use super::{all_ais, MonteCarloAi, Weights};

    /// [`all_ais`], MonteCarlo on a small budget: debug builds play its
    /// games slowly.
    fn quick_ais() -> Vec<Box<dyn TantrixAi>> {
        all_ais()
            .into_iter()
            .map(|ai| -> Box<dyn TantrixAi> {
                match ai.name() {
                    "MonteCarlo" => Box::new(MonteCarloAi::new(Weights::BALANCED, 4, 2)),
                    _ => ai,
                }
            })
            .collect()
    }

    #[test]
    fn seeded_ais_play_the_same_game_again() {
        for ai in quick_ais() {
            let play = || {
                let mut game = Game::new_seeded(
                    Player::Ai(ai.with_seed(1)),
                    Player::Ai(ai.with_seed(2)),
                    Player::None,
                    Player::None,
                    7,
                )
                .unwrap();
                game.advance();
                game.history().to_vec()
            };
            assert_eq!(play(), play(), "{}", ai.name());
        }
    }

    #[test]
    fn all_ais_choose_from_supplied_legal_moves() {
        for ai in quick_ais() {
            let game = Game::new(
                Player::Ai(ai.clone_box()),
                Player::Ai(ai.clone_box()),
                Player::None,
                Player::None,
            )
            .unwrap();
            let possible = game.legal_moves();
            let chosen = game.ai_choose().unwrap();
            assert!(
                possible.contains(&chosen),
                "{} returned a move outside the legal move set",
                ai.name()
            );
        }
    }
}
