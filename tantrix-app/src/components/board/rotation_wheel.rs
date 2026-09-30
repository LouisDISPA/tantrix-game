use dioxus::prelude::*;

use crate::components::tile::HexSvg;
use crate::hooks::use_phase_view;
use crate::state::actions::pick_rotation;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{Hovered, UiStateStoreExt};

const HEX_SIZE: f64 = 28.0;
const SVG_SIZE: f64 = HEX_SIZE * 2.2;

/// The radial menu of candidate rotations shown during SelectRotation phase.
///
/// Reads phase_view itself — renders nothing unless the wheel is currently
/// active. Hover updates the shared `hovered` signal so the board preview
/// reflects the current pick.
#[component]
pub fn RotationWheel() -> Element {
    let ctx = use_context::<GameCtx>();
    let mut hovered = ctx.ui_state.hovered();

    let phase_view = use_phase_view();
    let Some(wheel) = phase_view().wheel else {
        return rsx! {};
    };

    let tile = wheel.tile;

    rsx! {
        div { class: "rotation-bar",
            for rotation in wheel.rotations.iter() {
                {
                    let cx = SVG_SIZE / 2.0;
                    let cy = SVG_SIZE / 2.0;
                    let rot = *rotation;

                    rsx! {
                        div {
                            key: "{rot:?}",
                            class: "rotation-bar-slot",
                            "data-rotation": "{rot:?}",
                            role: "button",
                            tabindex: "0",
                            aria_label: "Choose rotation",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                pick_rotation(ctx, rot);
                            },
                            onkeydown: move |e| {
                                if matches!(e.key(), Key::Enter)
                                    || matches!(e.key(), Key::Character(c) if c == " ")
                                {
                                    pick_rotation(ctx, rot);
                                }
                            },
                            onmouseenter: move |_| hovered.set(Hovered::Rotation(rot)),
                            onmouseleave: move |_| hovered.set(Hovered::None),
                            svg {
                                view_box: "0 0 {SVG_SIZE:.0} {SVG_SIZE:.0}",
                                class: "rotation-bar-svg",
                                HexSvg {
                                    tile,
                                    rotation: rot,
                                    cx,
                                    cy,
                                    hex_size: HEX_SIZE,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
