use enum_map::EnumMap;
use tantrix_game::{
    AiView, Color, IndexedBoard, Move, PlayerScore, Position, ScoringPath, TantrixAi,
};

#[derive(Clone)]
pub struct SmartAi;

/// Score a candidate move with a multi-priority evaluation:
///   1. Own score gain (weight 1000) — primary objective
///   2. Opponent score delta (weight 100) — avoid feeding active opponents
///   3. Avoid extending opponent's path (weight -50) — don't help them
///
/// `scores_before` and `paths_before` are the board's own, worked out once
/// for every candidate.
fn evaluate(
    board: &IndexedBoard,
    scores_before: &EnumMap<Color, PlayerScore>,
    paths_before: &EnumMap<Color, ScoringPath>,
    m: &Move,
    player: Color,
    opponent: Color,
) -> i64 {
    let scores_after = board.scores_after(scores_before, m);

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
fn extends_opponent_path(pos: Position, opp_path: &ScoringPath) -> bool {
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

    fn choose_move(&self, view: &AiView) -> Move {
        let current_player = view.current_player;
        let scores = view.board.scores();
        let paths = view.board.scoring_paths();
        let board = view.board.indexed();
        let opponent = view
            .playing_colors
            .iter()
            .copied()
            .filter(|&c| c != current_player)
            .max_by_key(|&c| scores[c].score)
            .unwrap_or(current_player);

        *view
            .legal_moves
            .iter()
            .max_by_key(|m| evaluate(&board, &scores, &paths, m, current_player, opponent))
            .unwrap()
    }
}
