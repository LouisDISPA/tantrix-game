use dioxus::prelude::*;
use tantrix_game::{Color, Rotation, Tile};

use crate::svg::paths::{
    color_css_class, color_link_path, color_name, find_color_pairs, hex_polygon_points,
};

/// Shared inner SVG content for every hex tile in the app: `tile` turned by
/// `rotation`, centered on (`cx`, `cy`).
///
/// Emits (in order, as SVG siblings):
/// 1. A `<g class="{base_class}">` wrapping the fill polygon + 3 color curves.
///    The wrapper lets CSS dim the whole tile independently of highlights
///    (e.g. `.hex-tile.dimmed-tile .tile-base` on game over).
/// 2. Optional scoring-path overlay curves, one per color in `highlight_colors`.
///    Rendered outside the base group so they stay bright when dimmed.
///
/// Pure primitive — takes plain values (no `ReadSignal` wrappers) and does not
/// read context. Callers provide their own outer `<svg>` / viewBox.
#[component]
pub fn HexSvg(
    tile: Tile,
    rotation: Rotation,
    cx: f64,
    cy: f64,
    hex_size: f64,
    #[props(default)] base_class: String,
    #[props(default)] highlight_colors: Vec<Color>,
) -> Element {
    let points = hex_polygon_points(cx, cy, hex_size);
    let pairs = find_color_pairs(tile, rotation);

    rsx! {
        g { class: base_class,
            polygon { points: points, class: "hex-tile-fill" }
            for (d1 , d2 , color) in pairs.iter().copied() {
                {
                    let path_d = color_link_path(cx, cy, d1, d2, hex_size);
                    let css_class = color_css_class(color);
                    rsx! {
                        path { key: "{color:?}-{d1:?}-{d2:?}", d: path_d, class: "hex-curve {css_class}" }
                    }
                }
            }
        }
        for (d1 , d2 , color) in pairs.iter().copied() {
            if highlight_colors.contains(&color) {
                {
                    let path_d = color_link_path(cx, cy, d1, d2, hex_size);
                    let name = color_name(color);
                    rsx! {
                        path { key: "score-{color:?}-{d1:?}-{d2:?}", d: path_d, class: "scoring-curve scoring-curve-{name}" }
                    }
                }
            }
        }
    }
}
