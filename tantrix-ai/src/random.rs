use enum_map::EnumMap;
use rand::prelude::IndexedRandom;
use tantrix_game::{Board, Color, Hand, Move, TantrixAi};

#[derive(Clone)]
pub struct RandomAi;

impl TantrixAi for RandomAi {
    fn name(&self) -> &str {
        "Random"
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn choose_move(
        &self,
        _board: &Board,
        _hands: &EnumMap<Color, Hand>,
        _current_player: Color,
        _active_players: &[Color],
        possible_moves: &[Move],
    ) -> Move {
        let mut rng = rand::rng();
        *possible_moves
            .choose(&mut rng)
            .expect("possible_moves must not be empty")
    }
}
