use dioxus::prelude::*;

use crate::components::tile::{BoardTile, BoardTileVariant, EmptyHex};
use crate::hooks::use_phase_view;
use crate::svg::geometry::HEX_SIZE;

/// Renders the click-target empty hexes for the current turn phase, plus the
/// ghost preview of the move being considered.
#[component]
pub fn BoardEmptyLayer() -> Element {
    let view = use_phase_view()();

    rsx! {
        for vp in view.positions.iter() {
            EmptyHex {
                key: "e{vp.pos.x()},{vp.pos.y()}",
                position: vp.pos,
                hex_size: HEX_SIZE,
                forced: vp.forced,
                faint: vp.faint,
            }
        }
        if let Some(preview) = view.preview {
            BoardTile {
                m: preview,
                hex_size: HEX_SIZE,
                variant: BoardTileVariant::Preview,
            }
        }
    }
}
