use enum_map::EnumMap;
use tantrix_game::{Board, Color, Move, TantrixAi, Tile};

#[derive(Clone)]
pub struct DefensiveAi;

impl TantrixAi for DefensiveAi {
    fn name(&self) -> &str {
        "Defensive"
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
                let scores = test.scores();
                let own = scores[current_player].score as i64;
                let best_opponent = scores
                    .iter()
                    .filter(|(c, _)| *c != current_player)
                    .map(|(_, s)| s.score as i64)
                    .max()
                    .unwrap_or(0);
                own - best_opponent
            })
            .unwrap()
    }
}
