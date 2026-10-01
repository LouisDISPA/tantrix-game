use std::collections::BTreeMap;

use dioxus::prelude::*;
use tantrix_game::{Color, Position};

use crate::state::game_state::GameCtx;
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};

/// Scoring-path overlay shown on game over. Maps each position to the list of
/// colors whose winning line/loop passes through it (usually 0 or 1 color, but
/// paths can cross so a position may appear under multiple colors).
///
/// When the user clicks or hovers a color in the scores panel, only that
/// color's path is returned; otherwise every player's path is included.
///
/// Call it once for the whole board (`BoardTilesLayer`) and hand each tile
/// its colors: the board is scored once per game over, and hovering a score
/// row only filters the paths already found.
pub fn use_highlight_map() -> Memo<BTreeMap<Position, Vec<Color>>> {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    // Each playing color's scoring path, in turn order; none before game over.
    let paths = use_memo(move || {
        let phase = ui_state.phase()();
        if phase != TurnPhase::GameOver {
            return Vec::new();
        }
        let gs = ctx.game_state.read();
        let mut paths = gs.scoring_paths();
        gs.playing_colors()
            .iter()
            .map(|&color| (color, std::mem::take(&mut paths[color].positions)))
            .collect::<Vec<(Color, Vec<Position>)>>()
    });

    use_memo(move || {
        let paths = paths.read();
        if paths.is_empty() {
            // Not game over: no overlay, whatever is hovered.
            return BTreeMap::new();
        }
        let hovered_sc = ui_state.hovered()().scoring_color();
        let highlighted_c = ui_state.highlighted_color()();
        let hl = hovered_sc.or(highlighted_c);
        let mut map = BTreeMap::<Position, Vec<Color>>::new();
        for (color, positions) in paths.iter() {
            let show = hl.is_none() || hl == Some(*color);
            if show {
                for &pos in positions {
                    map.entry(pos).or_default().push(*color);
                }
            }
        }
        map
    })
}
