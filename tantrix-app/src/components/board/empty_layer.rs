use dioxus::prelude::*;
use tantrix_game::Placement;

use crate::components::tile::{BoardTile, BoardTileVariant, EmptyHex};
use crate::hooks::phase_view::ValidPosition;
use crate::svg::geometry::HEX_SIZE;

/// Renders the click-target empty hexes for the current turn phase.
///
/// `positions` comes from the board (`use_valid_positions`): this layer
/// re-renders when the set of hexes changes, not when one is hovered.
#[component]
pub fn BoardEmptyLayer(positions: ReadSignal<Vec<ValidPosition>>) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("BoardEmptyLayer", ());

    rsx! {
        for vp in positions.iter() {
            EmptyHex {
                key: "e{vp.pos.x()},{vp.pos.y()}",
                position: vp.pos,
                hex_size: HEX_SIZE,
                forced: vp.forced,
                faint: vp.faint,
            }
        }
    }
}

/// The ghost preview of the move being considered, drawn over the empty
/// hexes. Its own component, so hovering re-renders the ghost alone.
#[component]
pub fn BoardPreview(placement: ReadSignal<Option<Placement>>) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("BoardPreview", ());

    rsx! {
        if let Some(preview) = placement() {
            BoardTile {
                placement: preview,
                hex_size: HEX_SIZE,
                variant: BoardTileVariant::Preview,
            }
        }
    }
}
