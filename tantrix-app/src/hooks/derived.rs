use dioxus::prelude::*;
use tantrix_game::{Color, Move};

use crate::state::game_state::GameCtx;
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};

/// Memoize the list of placed moves so components that iterate them don't
/// re-clone the board every render. Recomputes whenever `game_state` changes.
pub fn use_placed_moves() -> Memo<Vec<Move>> {
    let ctx = use_context::<GameCtx>();
    use_memo(move || ctx.game_state.read().board().iter().copied().collect())
}

/// The color whose turn it currently is.
pub fn use_current_player() -> Memo<Color> {
    let ctx = use_context::<GameCtx>();
    use_memo(move || ctx.game_state.read().current_player())
}

/// True once the UI has entered the terminal GameOver phase.
pub fn use_is_game_over() -> Memo<bool> {
    let ctx = use_context::<GameCtx>();
    use_memo(move || ctx.ui_state.phase()() == TurnPhase::GameOver)
}

/// True when *any* tile in the current player's hand has a forced move.
/// StatusBar uses this to toggle the urgent banner styling; extracting it
/// keeps the scan off the render hot path.
pub fn use_has_forced_move() -> Memo<bool> {
    let ctx = use_context::<GameCtx>();
    use_memo(move || {
        ctx.game_state
            .read()
            .hand_info()
            .iter()
            .any(|info| info.has_forced)
    })
}
