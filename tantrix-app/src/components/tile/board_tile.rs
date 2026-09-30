use dioxus::prelude::*;
use tantrix_game::Move;

use crate::components::tile::hex_svg::HexSvg;
use crate::hooks::{use_highlight_map, use_is_game_over};
use crate::state::game_state::GameCtx;
use crate::state::ui_state::UiStateStoreExt;
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
#[component]
pub fn BoardTile(m: Move, hex_size: f64, variant: BoardTileVariant) -> Element {
    let ctx = use_context::<GameCtx>();
    let highlight_map = use_highlight_map();

    let highlight_colors = use_memo(move || {
        highlight_map
            .read()
            .get(&m.position)
            .cloned()
            .unwrap_or_default()
    });
    let is_game_over = use_is_game_over();
    let just_placed = use_memo(move || ctx.ui_state.last_placed_position()() == Some(m.position));

    let style = hex_position_style(m.position, hex_size);
    let is_preview = matches!(variant, BoardTileVariant::Preview);

    rsx! {
        div {
            class: "board-tile hex-tile",
            class: if is_preview { "tile-preview" },
            class: if just_placed() { "just-placed" },
            class: if is_game_over() { "dimmed-tile" },
            style: "{style}",
            "data-position": "{m.position.x()},{m.position.y()}",
            svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                HexSvg {
                    m,
                    cx: TILE_CX,
                    cy: TILE_CY,
                    hex_size: TILE_HEX,
                    base_class: "tile-base".to_string(),
                    highlight_colors: highlight_colors(),
                }
            }
        }
    }
}
