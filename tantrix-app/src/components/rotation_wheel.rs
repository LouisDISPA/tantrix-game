use dioxus::prelude::*;
use tantrix_game::{Move, Position, Rotation, Tile};

use crate::components::tile::HexSvg;
use crate::state::game_state::GameContext;
use crate::state::ui_state::{Hovered, UiStateStoreExt};

const HEX_SIZE: f64 = 28.0;
const SVG_SIZE: f64 = HEX_SIZE * 2.2;

#[component]
pub fn RotationWheel(
    tile: Tile,
    position: Position,
    rotations: Vec<Rotation>,
    on_select: EventHandler<Rotation>,
) -> Element {
    let ctx = use_context::<GameContext>();
    let mut hovered = ctx.ui_state.hovered();

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
                    let cx = SVG_SIZE / 2.0;
                    let cy = SVG_SIZE / 2.0;
                    let rot = *rotation;

                    rsx! {
                        div {
                            class: "rotation-bar-slot",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                on_select.call(rot);
                            },
                            onmouseenter: move |_| {
                                hovered.set(Hovered::Rotation(rot));
                            },
                            onmouseleave: move |_| {
                                hovered.set(Hovered::None);
                            },
                            svg {
                                view_box: "0 0 {SVG_SIZE:.0} {SVG_SIZE:.0}",
                                class: "rotation-bar-svg",
                                HexSvg { m, cx, cy, hex_size: HEX_SIZE }
                            }
                        }
                    }
                }
            }
        }
    }
}
