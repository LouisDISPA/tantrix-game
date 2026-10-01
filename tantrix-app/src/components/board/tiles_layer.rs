use dioxus::prelude::*;
use tantrix_game::Move;

use crate::components::tile::{BoardTile, BoardTileVariant};
use crate::hooks::use_highlight_map;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::UiStateStoreExt;
use crate::svg::geometry::HEX_SIZE;

/// Renders every placed move on the board with optional just-placed animation
/// and scoring-path highlighting. The scoring overlay and the last placed
/// position are worked out once here for the whole board; each tile gets
/// only its own colors and just-placed flag, and `dimmed` once the game is
/// over, so a tile re-renders only when one of those changes.
#[component]
pub fn BoardTilesLayer(placed_moves: ReadSignal<Vec<Move>>, dimmed: bool) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("BoardTilesLayer", ());
    let ctx = use_context::<GameCtx>();
    let highlight_map = use_highlight_map();
    let last_placed = use_memo(move || ctx.ui_state.last_placed_position()());

    // Each tile is the loop's direct child, so its key is the list's key: a
    // move inserts one tile (wherever its position sorts) and keeps every
    // other tile mounted. (A `{ block }` body would wrap each tile in an
    // unkeyed node, and the list would be diffed by index: every tile after
    // the new one remounted.)
    rsx! {
        for m in placed_moves.iter() {
            BoardTile {
                key: "{m.position.x()},{m.position.y()}",
                placement: m.placement(),
                hex_size: HEX_SIZE,
                variant: BoardTileVariant::Placed,
                highlight_colors: highlight_map.read().get(&m.position).cloned().unwrap_or_default(),
                just_placed: last_placed() == Some(m.position),
                dimmed,
            }
        }
    }
}
