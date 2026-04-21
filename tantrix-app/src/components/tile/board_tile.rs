use dioxus::prelude::*;
use tantrix_game::{Color, Move};

use crate::components::tile::hex_svg::HexSvg;
use crate::svg::geometry::{hex_position_style, TILE_CX, TILE_CY, TILE_HEX, TILE_VB_H, TILE_VB_W};

/// What kind of board tile is being rendered.
///
/// `Placed` is a committed move already on the board (may be dimmed/highlighted);
/// `Preview` is the translucent ghost shown before the move is confirmed.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum BoardTileVariant {
    #[default]
    Placed,
    Preview,
}

/// A tile rendered at its board position. Absolutely positioned via
/// [`hex_position_style`] using the tile's axial coordinates.
#[component]
pub fn BoardTile(
    m: ReadSignal<Move>,
    hex_size: ReadSignal<f64>,
    #[props(default)] variant: ReadSignal<BoardTileVariant>,
    #[props(default)] just_placed: ReadSignal<bool>,
    #[props(default)] dimmed: ReadSignal<bool>,
    #[props(default)] highlight_colors: ReadSignal<Vec<Color>>,
) -> Element {
    let m = m();
    let hex_size = hex_size();
    let variant = variant();
    let just_placed = just_placed();
    let dimmed = dimmed();
    let highlight_colors = highlight_colors();

    let style = hex_position_style(m.position, hex_size);

    let class = {
        let mut c = String::from("board-tile hex-tile");
        match variant {
            BoardTileVariant::Placed => {}
            BoardTileVariant::Preview => c.push_str(" tile-preview"),
        }
        if just_placed {
            c.push_str(" just-placed");
        }
        if dimmed {
            c.push_str(" dimmed-tile");
        }
        c
    };

    rsx! {
        div { class: "{class}", style: "{style}",
            svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                HexSvg {
                    m,
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
