use tantrix_game::{AiView, Move, TantrixAi};

#[derive(Clone)]
pub struct DefensiveAi;

impl TantrixAi for DefensiveAi {
    fn name(&self) -> &str {
        "Defensive"
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn choose_move(&self, view: &AiView) -> Move {
        let (board, current_player) = (view.board.indexed(), view.current_player);
        let before = view.board.scores();
        *view
            .legal_moves
            .iter()
            .max_by_key(|m| {
                let scores = board.scores_after(&before, m);
                let own = scores[current_player].score as i64;
                let best_opponent = view
                    .playing_colors
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
