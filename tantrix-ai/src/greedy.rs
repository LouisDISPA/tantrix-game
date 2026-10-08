use tantrix_game::{AiView, Move, TantrixAi};

#[derive(Clone)]
pub struct GreedyAi;

impl TantrixAi for GreedyAi {
    fn name(&self) -> &str {
        "Greedy"
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn choose_move(&self, view: &AiView) -> Move {
        let board = view.board.indexed();
        let score = view.board.scores()[view.current_player].score;
        *view
            .legal_moves
            .iter()
            .max_by_key(|m| board.score_after(score, m, view.current_player))
            .unwrap()
    }
}
