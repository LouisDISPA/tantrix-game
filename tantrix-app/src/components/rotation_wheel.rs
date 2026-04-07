use dioxus::prelude::*;
use tantrix_game::{Move, Position, Rotation, Tile};

use crate::state::game_state::GameContext;
use crate::state::ui_state::UiStateStoreExt;
use crate::svg::paths::{color_css_class, color_link_path, find_color_pairs, hex_polygon_points};

const HEX_SIZE: f64 = 28.0;
const SVG_SIZE: f64 = HEX_SIZE * 2.2;

#[component]
pub fn RotationWheel(
    tile: Tile,
    position: Position,
    rotations: Vec<Rotation>,
    on_select: EventHandler<Rotation>,
    on_cancel: EventHandler<()>,
) -> Element {
    let ctx = use_context::<GameContext>();
    // Only access hovered_rotation field — won't subscribe to anything else
    let mut hovered_rot = ctx.ui_state.hovered_rotation();

    rsx! {
        div { class: "rotation-bar",
            for rotation in rotations.iter() {
                {
                    let m = Move {
                        tile,
                        position,
                        rotation: *rotation,
                        forced: false,
                    };
                    let pairs = find_color_pairs(&m);
                    let cx = SVG_SIZE / 2.0;
                    let cy = SVG_SIZE / 2.0;
                    let points = hex_polygon_points(cx, cy, HEX_SIZE);

                    let rot = *rotation;

                    rsx! {
                        div {
                            class: "rotation-bar-slot",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                on_select.call(rot);
                            },
                            onmouseenter: move |_| {
                                hovered_rot.set(Some(rot));
                            },
                            onmouseleave: move |_| {
                                hovered_rot.set(None);
                            },
                            svg {
                                view_box: "0 0 {SVG_SIZE:.0} {SVG_SIZE:.0}",
                                class: "rotation-bar-svg",
                                polygon { points: "{points}", class: "hex-tile-fill" }
                                for (d1 , d2 , color) in pairs {
                                    {
                                        let path_d = color_link_path(cx, cy, d1, d2, HEX_SIZE);
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
            }
        }
    }
}
