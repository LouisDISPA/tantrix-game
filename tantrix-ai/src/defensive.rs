use enum_map::EnumMap;
use tantrix_game::{Board, Color, Hand, Move, TantrixAi};

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
        _hands: &EnumMap<Color, Hand>,
        current_player: Color,
        active_players: &[Color],
        possible_moves: &[Move],
    ) -> Move {
        let before = board.scores();
        *possible_moves
            .iter()
            .max_by_key(|m| {
                let scores = board.scores_after(&before, m);
                let own = scores[current_player].score as i64;
                let best_opponent = active_players
                    .iter()
                    .copied()
                    .filter(|&c| c != current_player)
                    .map(|c| scores[c].score as i64)
                    .max()
                    .unwrap_or(0);
                own - best_opponent
            })
            .unwrap()
    }
}
