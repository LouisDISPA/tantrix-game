use dioxus::prelude::*;
use tantrix_game::Move;

use crate::svg::geometry::{hex_position_style, TILE_CX, TILE_CY, TILE_HEX, TILE_VB_H, TILE_VB_W};
use crate::svg::paths::{color_css_class, color_link_path, find_color_pairs, hex_polygon_points};

/// Ghost preview tile: renders as an absolutely-positioned div with inline SVG.
#[component]
pub fn TilePreview(m: ReadSignal<Move>, hex_size: ReadSignal<f64>) -> Element {
    let m = m();
    let hex_size = hex_size();

    let style = hex_position_style(m.position, hex_size);

    let points = hex_polygon_points(TILE_CX, TILE_CY, TILE_HEX);
    let pairs = find_color_pairs(&m);

    rsx! {
        div { class: "board-tile tile-preview", style: "{style}",
            svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                polygon { points: "{points}", class: "hex-tile-fill" }
                for (d1 , d2 , color) in pairs {
                    {
                        let path_d = color_link_path(TILE_CX, TILE_CY, d1, d2, TILE_HEX);
                        let css_class = color_css_class(color);
                        rsx! {
                            path { d: "{path_d}", class: "hex-curve {css_class}" }
                        }
                    }
                }
            }
        }
    }
}
