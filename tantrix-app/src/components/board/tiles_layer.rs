use dioxus::prelude::*;
use tantrix_game::Move;

use crate::components::tile::{BoardTile, BoardTileVariant};
use crate::hooks::use_highlight_map;
use crate::svg::geometry::HEX_SIZE;

/// Renders every placed move on the board with optional just-placed animation
/// and scoring-path highlighting. The scoring overlay is worked out once here
/// for the whole board; each tile gets only its own colors, and `dimmed`
/// once the game is over.
#[component]
pub fn BoardTilesLayer(placed_moves: ReadSignal<Vec<Move>>, dimmed: bool) -> Element {
    let highlight_map = use_highlight_map();

    rsx! {
        for m in placed_moves.iter() {
            {
                let highlight_colors = highlight_map
                    .read()
                    .get(&m.position)
                    .cloned()
                    .unwrap_or_default();
                rsx! {
                    BoardTile {
                        key: "{m.position.x()},{m.position.y()}",
                        placement: m.placement(),
                        hex_size: HEX_SIZE,
                        variant: BoardTileVariant::Placed,
                        highlight_colors,
                        dimmed,
                    }
                }
            }
        }
    }
}
