use enum_map::EnumMap;
use tantrix_game::{Board, Color, Move, TantrixAi, Tile};

#[derive(Clone)]
pub struct FirstMoveAi;

impl TantrixAi for FirstMoveAi {
    fn name(&self) -> &str {
        "FirstMove"
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn choose_move(
        &self,
        _board: &Board,
        _hands: EnumMap<Color, &[Tile]>,
        _current_player: Color,
        possible_moves: &[Move],
    ) -> Move {
        possible_moves[0]
    }
}
