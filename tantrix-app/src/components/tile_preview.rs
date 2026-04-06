use dioxus::prelude::*;
use tantrix_game::Move;

use crate::svg::geometry::axial_to_pixel;
use crate::svg::paths::{color_css_class, color_link_path, find_color_pairs, hex_polygon_points};

#[component]
pub fn TilePreview(m: Move, hex_size: f64) -> Element {
    let (cx, cy) = axial_to_pixel(m.position, hex_size);
    let points = hex_polygon_points(cx, cy, hex_size);
    let pairs = find_color_pairs(&m);

    rsx! {
        g { class: "tile-preview",
            polygon { points: "{points}" }
            for (d1, d2, color) in pairs {
                {
                    let path_d = color_link_path(cx, cy, d1, d2, hex_size);
                    let css_class = color_css_class(color);
                    rsx! {
                        path {
                            d: "{path_d}",
                            class: "hex-curve {css_class}",
                        }
                    }
                }
            }
        }
    }
}
