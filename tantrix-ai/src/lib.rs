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
        Box::new(RandomAi::new()),
        Box::new(GreedyAi),
        Box::new(DefensiveAi),
        Box::new(LoopHunterAi),
        Box::new(SmartAi),
    ]
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Game, Player};

    use super::all_ais;

    #[test]
    fn seeded_ais_play_the_same_game_again() {
        for ai in all_ais() {
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
        for ai in all_ais() {
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
