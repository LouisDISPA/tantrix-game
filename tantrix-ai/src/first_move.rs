use tantrix_game::{AiView, Move, TantrixAi};

#[derive(Clone)]
pub struct FirstMoveAi;

impl TantrixAi for FirstMoveAi {
    fn name(&self) -> &str {
        "FirstMove"
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn choose_move(&self, view: &AiView) -> Move {
        view.legal_moves[0]
    }
}
