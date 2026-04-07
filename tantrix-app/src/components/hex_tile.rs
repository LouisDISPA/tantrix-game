use dioxus::prelude::*;
use tantrix_game::{Color, Move};

use crate::svg::geometry::{hex_position_style, TILE_CX, TILE_CY, TILE_HEX, TILE_VB_H, TILE_VB_W};
use crate::svg::paths::{color_css_class, color_link_path, color_name, find_color_pairs, hex_polygon_points};

/// Board tile: renders as an absolutely-positioned div containing an inline SVG.
#[component]
pub fn HexTile(
    m: ReadSignal<Move>,
    hex_size: ReadSignal<f64>,
    #[props(default)] just_placed: ReadSignal<bool>,
    #[props(default)] extra_class: ReadSignal<String>,
    #[props(default)] highlight_colors: ReadSignal<Vec<Color>>,
) -> Element {
    let m = m();
    let hex_size = hex_size();
    let just_placed = just_placed();
    let extra_class = extra_class();
    let highlight_colors = highlight_colors();

    let style = hex_position_style(m.position, hex_size);

    let class = if just_placed {
        format!("board-tile hex-tile just-placed {extra_class}")
    } else {
        format!("board-tile hex-tile {extra_class}")
    };

    let points = hex_polygon_points(TILE_CX, TILE_CY, TILE_HEX);
    let pairs = find_color_pairs(&m);

    rsx! {
        div { class: "{class}", style: "{style}",
            svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                g { class: "tile-base",
                    polygon { points: "{points}", class: "hex-tile-fill" }
                    for (d1 , d2 , color) in pairs.iter().copied() {
                        {
                            let path_d = color_link_path(TILE_CX, TILE_CY, d1, d2, TILE_HEX);
                            let css_class = color_css_class(color);
                            rsx! {
                                path { d: "{path_d}", class: "hex-curve {css_class}" }
                            }
                        }
                    }
                }
                for (d1 , d2 , color) in pairs.iter().copied() {
                    if highlight_colors.contains(&color) {
                        {
                            let path_d = color_link_path(TILE_CX, TILE_CY, d1, d2, TILE_HEX);
                            let name = color_name(color);
                            rsx! {
                                path { d: "{path_d}", class: "scoring-curve scoring-curve-{name}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a hex tile at a specific pixel position (for hand tiles, rotation wheel, etc.)
/// Outputs SVG `<g>` group — caller must wrap in their own `<svg>`.
#[component]
pub fn HexTileAtPixel(
    m: ReadSignal<Move>,
    cx: ReadSignal<f64>,
    cy: ReadSignal<f64>,
    hex_size: ReadSignal<f64>,
    #[props(default)] extra_class: ReadSignal<String>,
) -> Element {
    let m = m();
    let cx = cx();
    let cy = cy();
    let hex_size = hex_size();
    let extra_class = extra_class();

    let points = hex_polygon_points(cx, cy, hex_size);
    let pairs = find_color_pairs(&m);

    rsx! {
        g { class: "hex-tile {extra_class}",
            polygon { points: "{points}" }
            for (d1 , d2 , color) in pairs {
                {
                    let path_d = color_link_path(cx, cy, d1, d2, hex_size);
                    let css_class = color_css_class(color);
                    rsx! {
                        path { d: "{path_d}", class: "hex-curve {css_class}" }
                    }
                }
            }
        }
    }
}
