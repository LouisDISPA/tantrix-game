use dioxus::prelude::*;
use tantrix_game::Position;

use crate::svg::geometry::{hex_position_style, TILE_CX, TILE_CY, TILE_HEX, TILE_VB_H, TILE_VB_W};
use crate::svg::paths::hex_polygon_points;

/// Empty hex position: renders as an absolutely-positioned div with inline SVG.
#[component]
pub fn HexEmpty(
    position: Position,
    hex_size: f64,
    #[props(default = false)] forced: bool,
    #[props(default = false)] faint: bool,
    #[props(default = String::new())] extra_class: String,
    onclick: Option<EventHandler<Position>>,
    onmouseenter: Option<EventHandler<Position>>,
    onmouseleave: Option<EventHandler<Position>>,
) -> Element {
    let style = hex_position_style(position, hex_size);

    let class = {
        let mut c = String::from("board-empty hex-empty");
        if forced { c.push_str(" forced"); }
        if faint { c.push_str(" faint"); }
        if !extra_class.is_empty() { c.push(' '); c.push_str(&extra_class); }
        c
    };

    let points = hex_polygon_points(TILE_CX, TILE_CY, TILE_HEX);

    rsx! {
        div {
            class: "{class}",
            style: "{style}",
            onclick: move |_| { if let Some(h) = &onclick { h.call(position); } },
            onmouseenter: move |_| { if let Some(h) = &onmouseenter { h.call(position); } },
            onmouseleave: move |_| { if let Some(h) = &onmouseleave { h.call(position); } },
            svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                polygon { points: "{points}" }
            }
        }
    }
}
