use tantrix_game::{AiView, Move, TantrixAi};

#[derive(Clone)]
pub struct LoopHunterAi;

impl TantrixAi for LoopHunterAi {
    fn name(&self) -> &str {
        "LoopHunter"
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn choose_move(&self, view: &AiView) -> Move {
        let board = view.board.indexed();
        let before = view.board.scores();
        *view
            .legal_moves
            .iter()
            .max_by_key(|m| {
                let scores = board.scores_after(&before, m)[view.current_player];
                scores.longest_loop * 3 + scores.longest_line
            })
            .unwrap()
    }
}
