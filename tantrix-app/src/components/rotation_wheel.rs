use std::f64::consts::PI;

use dioxus::prelude::*;
use tantrix_game::{Move, Position, Rotation, Tile};

use crate::svg::paths::{color_css_class, color_link_path, find_color_pairs, hex_polygon_points};

const SLOT_SIZE: f64 = 96.0;
const HEX_SIZE: f64 = 30.0;
const SVG_SIZE: f64 = HEX_SIZE * 2.2;

/// Compute (left, top) positions and overall (width, height) for the wheel
/// based on how many rotation choices there are.
fn layout_slots(count: usize) -> (Vec<(f64, f64)>, f64, f64) {
    if count == 2 {
        // Horizontal pair, closer together
        let gap = 20.0;
        let w = SLOT_SIZE * 2.0 + gap + 20.0;
        let h = SLOT_SIZE + 40.0;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let positions = vec![
            (cx - SLOT_SIZE / 2.0 - gap / 2.0, cy),
            (cx + SLOT_SIZE / 2.0 + gap / 2.0, cy),
        ];
        (positions, w, h)
    } else {
        // Circular layout
        let radius = 130.0;
        let size = (radius + SLOT_SIZE / 2.0) * 2.0 + 20.0;
        let center = size / 2.0;
        let positions = (0..count)
            .map(|i| {
                let angle = (i as f64 / count as f64) * 2.0 * PI - PI / 2.0;
                (
                    center + radius * angle.cos(),
                    center + radius * angle.sin(),
                )
            })
            .collect();
        (positions, size, size)
    }
}

#[component]
pub fn RotationWheel(
    tile: Tile,
    position: Position,
    rotations: Vec<Rotation>,
    on_select: EventHandler<Rotation>,
    on_cancel: EventHandler<()>,
) -> Element {
    let (slot_positions, wheel_w, wheel_h) = layout_slots(rotations.len());

    rsx! {
        div { class: "rotation-overlay",
            div {
                class: "rotation-backdrop",
                onclick: move |_| on_cancel.call(()),
            }
            div {
                class: "rotation-wheel",
                style: "width: {wheel_w:.0}px; height: {wheel_h:.0}px;",
                for (i, rotation) in rotations.iter().enumerate() {
                    {
                        let (left, top) = slot_positions[i];

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
                        let rot_label = match rot {
                            Rotation::None => "0°",
                            Rotation::R60 => "60°",
                            Rotation::R120 => "120°",
                            Rotation::R180 => "180°",
                            Rotation::R240 => "240°",
                            Rotation::R300 => "300°",
                        };

                        rsx! {
                            div {
                                class: "rotation-slot",
                                style: "left: {left:.0}px; top: {top:.0}px; width: {SLOT_SIZE:.0}px; height: {SLOT_SIZE:.0}px;",
                                onclick: move |evt| {
                                    evt.stop_propagation();
                                    on_select.call(rot);
                                },
                                svg {
                                    view_box: "0 0 {SVG_SIZE:.0} {SVG_SIZE:.0}",
                                    class: "rotation-slot-svg",
                                    polygon {
                                        points: "{points}",
                                        class: "hex-tile-fill",
                                    }
                                    for (d1, d2, color) in pairs {
                                        {
                                            let path_d = color_link_path(cx, cy, d1, d2, HEX_SIZE);
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
                                span { class: "rotation-label", "{rot_label}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
