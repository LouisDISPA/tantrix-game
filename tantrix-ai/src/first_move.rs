use enum_map::EnumMap;
use tantrix_game::{Board, Color, Hand, Move, TantrixAi};

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
        _hands: &EnumMap<Color, Hand>,
        _current_player: Color,
        _active_players: &[Color],
        possible_moves: &[Move],
    ) -> Move {
        possible_moves[0]
    }
}
