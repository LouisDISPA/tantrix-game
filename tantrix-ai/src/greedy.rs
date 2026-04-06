use enum_map::EnumMap;
use tantrix_game::{Board, Color, Move, TantrixAi, Tile};

#[derive(Clone)]
pub struct GreedyAi;

impl TantrixAi for GreedyAi {
    fn name(&self) -> &str {
        "Greedy"
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn choose_move(
        &self,
        board: &Board,
        _hands: EnumMap<Color, &[Tile]>,
        current_player: Color,
        possible_moves: &[Move],
    ) -> Move {
        *possible_moves
            .iter()
            .max_by_key(|m| {
                let mut test = board.clone();
                test.insert_move(**m).unwrap();
                test.scores()[current_player].score
            })
            .unwrap()
    }
}
