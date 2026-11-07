use dioxus::prelude::*;

use crate::svg::geometry::{TILE_CX, TILE_CY, TILE_VB_H, TILE_VB_W};
use crate::svg::paths::hex_polygon_points;

/// The keyboard's focus ring round a tile: a hex of `size` (in the tile
/// viewBox's units, so larger than `TILE_HEX` to sit outside it), drawn as
/// a halo under a line (`.focus-hex-*` in `main.css`). The wrapper decides
/// when it shows.
#[component]
pub fn FocusHex(size: f64, #[props(default)] class: &'static str) -> Element {
    let points = hex_polygon_points(TILE_CX, TILE_CY, size);
    rsx! {
        svg {
            class: "focus-hex {class}",
            view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
            "aria-hidden": "true",
            polygon { class: "focus-hex-halo", points: points.clone() }
            polygon { class: "focus-hex-line", points }
        }
    }
}
