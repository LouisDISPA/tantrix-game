use dioxus::prelude::*;

use crate::components::tile::BoardTile;
use crate::hooks::{use_highlight_map, use_is_game_over, use_placed_moves};
use crate::state::game_state::GameContext;
use crate::state::ui_state::UiStateStoreExt;
use crate::svg::geometry::HEX_SIZE;


/// Renders every placed move on the board with optional just-placed animation
/// and scoring-path highlighting.
#[component]
pub fn BoardTilesLayer() -> Element {
    let ctx = use_context::<GameContext>();
    let last_placed = ctx.ui_state.last_placed_position()();

    let placed_moves = use_placed_moves()();
    let hl_map = use_highlight_map()();
    let is_game_over = use_is_game_over()();

    rsx! {
        for m in placed_moves.iter() {
            {
                let colors = hl_map.get(&m.position).cloned().unwrap_or_default();
                let just_placed = last_placed == Some(m.position);
                rsx! {
                    BoardTile {
                        key: "{m.position.x()},{m.position.y()}",
                        m: *m,
                        hex_size: HEX_SIZE,
                        just_placed,
                        dimmed: is_game_over,
                        highlight_colors: colors,
                    }
                }
            }
        }
    }
}
