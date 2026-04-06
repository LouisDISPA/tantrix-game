use dioxus::prelude::*;
use tantrix_game::Move;

use crate::svg::geometry::axial_to_pixel;
use crate::svg::paths::{color_css_class, color_link_path, find_color_pairs, hex_polygon_points};

#[component]
pub fn HexTile(
    m: Move,
    hex_size: f64,
    #[props(default = false)] just_placed: bool,
    #[props(default = String::new())] extra_class: String,
) -> Element {
    let (cx, cy) = axial_to_pixel(m.position, hex_size);
    let points = hex_polygon_points(cx, cy, hex_size);
    let pairs = find_color_pairs(&m);

    let class = if just_placed {
        format!("hex-tile just-placed {extra_class}")
    } else {
        format!("hex-tile {extra_class}")
    };

    rsx! {
        g { class: "{class}",
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

/// Render a hex tile at a specific pixel position (for hand tiles, rotation wheel, etc.)
#[component]
pub fn HexTileAtPixel(
    m: Move,
    cx: f64,
    cy: f64,
    hex_size: f64,
    #[props(default = String::new())] extra_class: String,
) -> Element {
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
