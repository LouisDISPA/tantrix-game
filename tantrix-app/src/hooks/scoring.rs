use std::collections::BTreeMap;

use dioxus::prelude::*;
use tantrix_game::{Color, Position};

use crate::state::game_state::{GameContext, GameStateStoreExt};
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};

/// Scoring-path overlay shown on game over. Maps each position to the list of
/// colors whose winning line/loop passes through it (usually 0 or 1 color, but
/// paths can cross so a position may appear under multiple colors).
///
/// When the user clicks or hovers a color in the scores panel, only that
/// color's path is returned; otherwise every player's path is included.
pub fn use_highlight_map() -> Memo<BTreeMap<Position, Vec<Color>>> {
    let ctx = use_context::<GameContext>();
    let ui_state = ctx.ui_state;
    let game_state = ctx.game_state;

    use_memo(move || {
        let phase = ui_state.phase()();
        if phase != TurnPhase::GameOver {
            return BTreeMap::new();
        }
        let hovered_sc = ui_state.hovered()().scoring_color();
        let highlighted_c = ui_state.highlighted_color()();
        let hl = hovered_sc.or(highlighted_c);
        let board = game_state.board();
        let b = board.read();
        let paths = b.scoring_paths();
        let colors = game_state.turn_tracker().read().playing_colors().to_vec();
        let mut map = BTreeMap::<Position, Vec<Color>>::new();
        for color in &colors {
            let show = hl.is_none() || hl == Some(*color);
            if show && !paths[*color].positions.is_empty() {
                for &pos in &paths[*color].positions {
                    map.entry(pos).or_default().push(*color);
                }
            }
        }
        map
    })
}
