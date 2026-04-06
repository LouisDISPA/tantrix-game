use dioxus::prelude::*;
use tantrix_game::{Move, Position, Rotation, Tile};

use crate::svg::paths::{color_css_class, color_link_path, find_color_pairs, hex_polygon_points};

/// Fixed viewBox size for hand tiles — the SVG internal coordinate space.
/// Actual rendered size is controlled entirely by CSS.
const TILE_VB: f64 = 56.0;
const TILE_HEX: f64 = 22.0;
/// Slightly larger hex used for selection/forced outline rings.
const OUTLINE_HEX: f64 = 25.0;

#[component]
pub fn HandTile(
    tile: Tile,
    index: usize,
    #[props(default = false)] selected: bool,
    #[props(default = false)] disabled: bool,
    #[props(default = false)] forced: bool,
    #[props(default = false)] is_new: bool,
    onclick: Option<EventHandler<usize>>,
    onmouseenter: Option<EventHandler<usize>>,
    onmouseleave: Option<EventHandler<usize>>,
) -> Element {
    let cx = TILE_VB / 2.0;
    let cy = TILE_VB / 2.0;
    let points = hex_polygon_points(cx, cy, TILE_HEX);

    let m = Move {
        tile,
        position: Position::CENTER,
        rotation: Rotation::None,
        forced: false,
    };
    let pairs = find_color_pairs(&m);

    // Hex-shaped outline ring for selected / forced states
    let outline_points = hex_polygon_points(cx, cy, OUTLINE_HEX);
    let show_outline = selected || forced;
    let outline_class = if selected {
        "hand-hex-ring selected"
    } else {
        "hand-hex-ring forced"
    };

    let class = {
        let mut c = String::from("hand-tile");
        if selected {
            c.push_str(" selected");
        }
        if disabled {
            c.push_str(" disabled");
        }
        if forced {
            c.push_str(" forced");
        }
        if is_new {
            c.push_str(" new-tile");
        }
        c
    };

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| {
                if !disabled {
                    if let Some(handler) = &onclick {
                        handler.call(index);
                    }
                }
            },
            onmouseenter: move |_| {
                if let Some(handler) = &onmouseenter {
                    handler.call(index);
                }
            },
            onmouseleave: move |_| {
                if let Some(handler) = &onmouseleave {
                    handler.call(index);
                }
            },
            svg { view_box: "0 0 {TILE_VB:.0} {TILE_VB:.0}",
                // Hex-shaped selection / forced ring (behind the tile)
                if show_outline {
                    polygon { points: "{outline_points}", class: "{outline_class}" }
                }
                // Tile polygon — uses shared .hex-tile-fill class
                polygon { points: "{points}", class: "hex-tile-fill" }
                // Colored curves — identical to board tiles
                for (d1 , d2 , color) in pairs {
                    {
                        let path_d = color_link_path(cx, cy, d1, d2, TILE_HEX);
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
