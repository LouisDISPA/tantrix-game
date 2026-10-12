//! Small, annotated rule diagrams using the game's tile graphics.

use dioxus::prelude::*;
use tantrix_game::{Color, Direction, Move, Position};

use crate::components::tile::hex_svg::HexSvg;
use crate::svg::geometry::{axial_to_pixel, board_bounding_box, edge_midpoint, hex_vertices};
use crate::svg::paths::hex_polygon_points;

const HEX: f64 = 38.0;

#[derive(Clone, Copy, PartialEq)]
pub enum SpaceKind {
    Forced,
    Blocked,
}

#[derive(Clone, Copy, PartialEq)]
pub struct DiagramSpace {
    pub position: Position,
    pub label: &'static str,
    pub kind: SpaceKind,
}

#[component]
pub fn LearnBoard(
    moves: Vec<Move>,
    #[props(default)] spaces: Vec<DiagramSpace>,
    #[props(default)] rays: Vec<(Position, Position)>,
    #[props(default)] preview: Option<Move>,
    #[props(default)] highlight: Option<Color>,
    #[props(default)] matched_edge: Option<(Position, Direction)>,
    #[props(into)] label: String,
) -> Element {
    let mut positions: Vec<Position> = moves.iter().map(|m| m.position).collect();
    positions.extend(spaces.iter().map(|space| space.position));
    if let Some(m) = preview {
        positions.push(m.position);
    }
    let (left, top, right, bottom) =
        board_bounding_box(&positions, HEX).unwrap_or((-50.0, -50.0, 50.0, 50.0));
    let view_box = format!(
        "{} {} {} {}",
        left - 16.0,
        top - 16.0,
        right - left + 32.0,
        bottom - top + 32.0
    );

    rsx! {
        div { class: "surface surface-inset learn-board",
            svg { view_box, role: "img", "aria-label": label.clone(),
                title { {label.clone()} }
                for (index, (from, to)) in rays.iter().enumerate() {
                    line {
                        key: "ray-{index}",
                        class: "learn-controlled-ray",
                        x1: axial_to_pixel(*from, HEX).0,
                        y1: axial_to_pixel(*from, HEX).1,
                        x2: axial_to_pixel(*to, HEX).0,
                        y2: axial_to_pixel(*to, HEX).1,
                    }
                }
                for space in spaces {
                    SpaceMark { key: "{space.position:?}", space }
                }
                for m in moves {
                    g { key: "{m.position:?}",
                        {
                            let (x, y) = axial_to_pixel(m.position, HEX);
                            rsx! {
                                HexSvg {
                                    tile: m.tile,
                                    rotation: m.rotation,
                                    cx: x,
                                    cy: y,
                                    hex_size: HEX,
                                    base_class: if highlight.is_some() { "learn-tile-muted".to_string() } else { String::new() },
                                    highlight_colors: highlight.into_iter().collect(),
                                }
                            }
                        }
                    }
                }
                if let Some(m) = preview {
                    g { class: "learn-preview",
                        {
                            let (x, y) = axial_to_pixel(m.position, HEX);
                            rsx! {
                                HexSvg {
                                    tile: m.tile,
                                    rotation: m.rotation,
                                    cx: x,
                                    cy: y,
                                    hex_size: HEX,
                                }
                            }
                        }
                    }
                }
                if let Some((position, direction)) = matched_edge {
                    {
                        let (x, y) = axial_to_pixel(position, HEX);
                        let (cx, cy) = edge_midpoint(&hex_vertices(x, y, HEX), direction);
                        rsx! {
                            circle {
                                class: "learn-matched-edge",
                                cx,
                                cy,
                                r: "12",
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SpaceMark(space: DiagramSpace) -> Element {
    let (x, y) = axial_to_pixel(space.position, HEX);
    rsx! {
        g {
            class: match space.kind {
                SpaceKind::Forced => "learn-space is-forced",
                SpaceKind::Blocked => "learn-space is-blocked",
            },
            polygon { points: hex_polygon_points(x, y, HEX) }
            text {
                x,
                y,
                dominant_baseline: "central",
                text_anchor: "middle",
                "{space.label}"
            }
        }
    }
}
