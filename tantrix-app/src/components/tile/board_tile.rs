use dioxus::prelude::*;
use tantrix_game::{Color, Placement};

use crate::components::tile::hex_svg::HexSvg;
use crate::svg::geometry::{hex_position_style, TILE_CX, TILE_CY, TILE_HEX, TILE_VB_H, TILE_VB_W};

/// What kind of board tile is being rendered.
///
/// `Placed` is a committed move already on the board (may be dimmed/highlighted);
/// `Preview` is the translucent ghost shown before the move is confirmed.
#[derive(Debug, Clone, PartialEq)]
pub enum BoardTileVariant {
    Placed,
    Preview,
}

/// A tile rendered at its board position. Absolutely positioned via
/// [`hex_position_style`] using the tile's axial coordinates.
///
/// `highlight_colors`: the scoring paths through the tile, `just_placed` for
/// the last move's placement animation, and `dimmed` once the game is over;
/// the tiles layer works all three out once for every tile. Reads no state.
#[component]
pub fn BoardTile(
    placement: Placement,
    hex_size: f64,
    variant: BoardTileVariant,
    #[props(default)] highlight_colors: Vec<Color>,
    #[props(default)] just_placed: bool,
    #[props(default)] dimmed: bool,
) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("BoardTile", placement.position);

    let style = hex_position_style(placement.position, hex_size);
    let is_preview = matches!(variant, BoardTileVariant::Preview);

    rsx! {
        div {
            class: "board-tile hex-tile",
            class: if is_preview { "tile-preview" },
            class: if just_placed { "just-placed" },
            class: if dimmed { "dimmed-tile" },
            style,
            "data-position": "{placement.position.x()},{placement.position.y()}",
            svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                HexSvg {
                    tile: placement.tile,
                    rotation: placement.rotation,
                    cx: TILE_CX,
                    cy: TILE_CY,
                    hex_size: TILE_HEX,
                    base_class: "tile-base".to_string(),
                    highlight_colors,
                }
            }
        }
    }
}
