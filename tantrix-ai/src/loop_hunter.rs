use enum_map::EnumMap;
use tantrix_game::{Board, Color, Hand, Move, TantrixAi};

#[derive(Clone)]
pub struct LoopHunterAi;

impl TantrixAi for LoopHunterAi {
    fn name(&self) -> &str {
        "LoopHunter"
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn choose_move(
        &self,
        board: &Board,
        _hands: &EnumMap<Color, Hand>,
        current_player: Color,
        possible_moves: &[Move],
    ) -> Move {
        *possible_moves
            .iter()
            .max_by_key(|m| {
                let mut test = board.clone();
                test.insert_move(**m).unwrap();
                let scores = test.scores()[current_player];
                scores.longest_loop * 3 + scores.longest_line
            })
            .unwrap()
    }
}
