use dioxus::prelude::*;
use tantrix_game::{Color, Move};

use crate::svg::paths::{
    color_css_class, color_link_path, color_name, find_color_pairs, hex_polygon_points,
};

/// Shared inner SVG content for every hex tile in the app.
///
/// Emits (in order, as SVG siblings):
/// 1. A `<g class="{base_class}">` wrapping the fill polygon + 3 color curves.
///    The wrapper lets CSS dim the whole tile independently of highlights
///    (e.g. `.hex-tile.dimmed-tile .tile-base` on game over).
/// 2. Optional scoring-path overlay curves, one per color in `highlight_colors`.
///    Rendered outside the base group so they stay bright when dimmed.
///
/// Callers provide their own outer `<svg>` element and viewBox — this keeps
/// hand tiles, board tiles, and the rotation wheel free to pick their own
/// coordinate space.
#[component]
pub fn HexSvg(
    m: ReadSignal<Move>,
    cx: ReadSignal<f64>,
    cy: ReadSignal<f64>,
    hex_size: ReadSignal<f64>,
    #[props(default)] base_class: ReadSignal<String>,
    #[props(default)] highlight_colors: ReadSignal<Vec<Color>>,
) -> Element {
    let m = m();
    let cx = cx();
    let cy = cy();
    let hex_size = hex_size();
    let base_class = base_class();
    let highlights = highlight_colors();

    let points = hex_polygon_points(cx, cy, hex_size);
    let pairs = find_color_pairs(&m);

    rsx! {
        g { class: "{base_class}",
            polygon { points: "{points}", class: "hex-tile-fill" }
            for (d1 , d2 , color) in pairs.iter().copied() {
                {
                    let path_d = color_link_path(cx, cy, d1, d2, hex_size);
                    let css_class = color_css_class(color);
                    rsx! {
                        path { d: "{path_d}", class: "hex-curve {css_class}" }
                    }
                }
            }
        }
        for (d1 , d2 , color) in pairs.iter().copied() {
            if highlights.contains(&color) {
                {
                    let path_d = color_link_path(cx, cy, d1, d2, hex_size);
                    let name = color_name(color);
                    rsx! {
                        path { d: "{path_d}", class: "scoring-curve scoring-curve-{name}" }
                    }
                }
            }
        }
    }
}
