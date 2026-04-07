use enum_map::EnumMap;
use tantrix_game::{Board, Color, Hand, Move, Position, TantrixAi};

#[derive(Clone)]
pub struct SmartAi;

/// Score a candidate move with a multi-priority evaluation:
///   1. Own score gain (weight 1000) — primary objective
///   2. Opponent score reduction (weight 100) — block their best chain
///   3. Avoid extending opponent's path (weight -50) — don't help them
fn evaluate(board: &Board, m: &Move, player: Color, opponent: Color) -> i64 {
    let scores_before = board.scores();
    let paths_before = board.scoring_paths();

    let mut test = board.clone();
    test.insert_move(*m).unwrap();

    let scores_after = test.scores();

    let own_gain = scores_after[player].score as i64 - scores_before[player].score as i64;
    let opp_loss = scores_before[opponent].score as i64 - scores_after[opponent].score as i64;

    // Penalty if this move's position is adjacent to the opponent's path endpoints,
    // helping extend their chain
    let opp_extend_penalty = if extends_opponent_path(m.position, &paths_before[opponent]) {
        -50
    } else {
        0
    };

    own_gain * 1000 + opp_loss * 100 + opp_extend_penalty
}

/// Check if a position is adjacent to an endpoint of the opponent's best path,
/// meaning placing there could help them extend.
fn extends_opponent_path(pos: Position, opp_path: &tantrix_game::ScoringPath) -> bool {
    if opp_path.positions.is_empty() || opp_path.is_loop {
        return false;
    }
    let endpoints = [
        opp_path.positions[0],
        opp_path.positions[opp_path.positions.len() - 1],
    ];
    endpoints
        .iter()
        .any(|ep| ep.around().any(|(neighbor, _)| neighbor == pos))
}

impl TantrixAi for SmartAi {
    fn name(&self) -> &str {
        "Smart"
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
        let opponent = board
            .scores()
            .iter()
            .filter(|(c, _)| *c != current_player)
            .max_by_key(|(_, s)| s.score)
            .map(|(c, _)| c)
            .unwrap_or(current_player);

        *possible_moves
            .iter()
            .max_by_key(|m| evaluate(board, m, current_player, opponent))
            .unwrap()
    }
}
