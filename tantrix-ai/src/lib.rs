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

/// All available AI constructors for use in tournaments.
pub fn all_ais() -> Vec<Box<dyn TantrixAi>> {
    vec![
        Box::new(FirstMoveAi),
        Box::new(RandomAi),
        Box::new(GreedyAi),
        Box::new(DefensiveAi),
        Box::new(LoopHunterAi),
        Box::new(SmartAi),
    ]
}
