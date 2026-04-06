use dioxus::prelude::*;
use tantrix_game::Position;

use crate::svg::geometry::axial_to_pixel;
use crate::svg::paths::hex_polygon_points;

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
    let (cx, cy) = axial_to_pixel(position, hex_size);
    let points = hex_polygon_points(cx, cy, hex_size);

    let class = {
        let mut c = String::from("hex-empty");
        if forced {
            c.push_str(" forced");
        }
        if faint {
            c.push_str(" faint");
        }
        if !extra_class.is_empty() {
            c.push(' ');
            c.push_str(&extra_class);
        }
        c
    };

    rsx! {
        g {
            class: "{class}",
            onclick: move |_| {
                if let Some(handler) = &onclick {
                    handler.call(position);
                }
            },
            onmouseenter: move |_| {
                if let Some(handler) = &onmouseenter {
                    handler.call(position);
                }
            },
            onmouseleave: move |_| {
                if let Some(handler) = &onmouseleave {
                    handler.call(position);
                }
            },
            polygon { points: "{points}" }
        }
    }
}
