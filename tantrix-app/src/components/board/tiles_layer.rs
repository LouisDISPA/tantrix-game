use dioxus::prelude::*;

use crate::components::tile::{BoardTile, BoardTileVariant};
use crate::hooks::use_placed_moves;
use crate::svg::geometry::HEX_SIZE;

/// Renders every placed move on the board with optional just-placed animation
/// and scoring-path highlighting.
#[component]
pub fn BoardTilesLayer() -> Element {
    rsx! {
        for m in use_placed_moves().iter() {
            {
                rsx! {
                    BoardTile {
                        key: "{m.position.x()},{m.position.y()}",
                        placement: m.placement(),
                        hex_size: HEX_SIZE,
                        variant: BoardTileVariant::Placed,
                    }
                }
            }
        }
    }
}
