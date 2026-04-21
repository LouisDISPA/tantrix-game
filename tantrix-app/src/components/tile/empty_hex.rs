use dioxus::prelude::*;
use tantrix_game::Position;

use crate::svg::geometry::{hex_position_style, TILE_CX, TILE_CY, TILE_HEX, TILE_VB_H, TILE_VB_W};
use crate::svg::paths::hex_polygon_points;

/// An empty hex slot on the board. Positioned at a board coordinate and wired
/// up with click/hover handlers so the parent can react to placement attempts.
///
/// No tile content — just the hex polygon. `forced` and `faint` toggle CSS
/// classes that adjust the fill appearance.
#[component]
pub fn EmptyHex(
    position: ReadSignal<Position>,
    hex_size: ReadSignal<f64>,
    #[props(default)] forced: ReadSignal<bool>,
    #[props(default)] faint: ReadSignal<bool>,
    onclick: Option<EventHandler<Position>>,
    onmouseenter: Option<EventHandler<Position>>,
    onmouseleave: Option<EventHandler<Position>>,
) -> Element {
    let position = position();
    let hex_size = hex_size();
    let forced = forced();
    let faint = faint();

    let style = hex_position_style(position, hex_size);
    let points = hex_polygon_points(TILE_CX, TILE_CY, TILE_HEX);

    rsx! {
        div {
            class: "board-empty hex-empty",
            class: if forced { "forced" },
            class: if faint { "faint" },
            style: "{style}",
            onclick: move |_| {
                if let Some(h) = &onclick {
                    h.call(position);
                }
            },
            onmouseenter: move |_| {
                if let Some(h) = &onmouseenter {
                    h.call(position);
                }
            },
            onmouseleave: move |_| {
                if let Some(h) = &onmouseleave {
                    h.call(position);
                }
            },
            svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                polygon { points: "{points}" }
            }
        }
    }
}
