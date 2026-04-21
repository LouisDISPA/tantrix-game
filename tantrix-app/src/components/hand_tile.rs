use dioxus::prelude::*;
use tantrix_game::{Move, Position, Rotation, Tile};

use crate::state::ui_state::HandTileState;
use crate::svg::paths::{color_css_class, color_link_path, find_color_pairs, hex_polygon_points};

/// Fixed viewBox size for hand tiles — the SVG internal coordinate space.
/// Actual rendered size is controlled entirely by CSS.
const TILE_VB: f64 = 56.0;
const TILE_HEX: f64 = 22.0;
/// Slightly larger hex used for selection/forced outline rings.
const OUTLINE_HEX: f64 = 25.0;

/// Interaction events a hand tile can raise. The tile's own hand index is
/// attached so the parent doesn't need per-index closures.
#[derive(Debug, Clone, Copy)]
pub enum HandTileAction {
    Click(usize),
    HoverEnter(usize),
    HoverLeave,
}

#[component]
pub fn HandTile(
    tile: ReadSignal<Tile>,
    index: ReadSignal<usize>,
    #[props(default)] state: ReadSignal<HandTileState>,
    #[props(default)] just_drawn: ReadSignal<bool>,
    on_action: EventHandler<HandTileAction>,
) -> Element {
    let tile = tile();
    let index = index();
    let state = state();
    let just_drawn = just_drawn();

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

    let show_outline = matches!(state, HandTileState::Selected | HandTileState::Forced);
    let outline_class = match state {
        HandTileState::Selected => "hand-hex-ring selected",
        _ => "hand-hex-ring forced",
    };
    let outline_points = hex_polygon_points(cx, cy, OUTLINE_HEX);

    let class = {
        let mut c = String::from("hand-tile");
        let state_class = state.css_class();
        if !state_class.is_empty() {
            c.push(' ');
            c.push_str(state_class);
        }
        if just_drawn {
            c.push_str(" new-tile");
        }
        c
    };

    let interactive = state.is_interactive();

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| {
                if interactive {
                    on_action.call(HandTileAction::Click(index));
                }
            },
            onmouseenter: move |_| on_action.call(HandTileAction::HoverEnter(index)),
            onmouseleave: move |_| on_action.call(HandTileAction::HoverLeave),
            svg { view_box: "0 0 {TILE_VB:.0} {TILE_VB:.0}",
                if show_outline {
                    polygon { points: "{outline_points}", class: "{outline_class}" }
                }
                polygon { points: "{points}", class: "hex-tile-fill" }
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
