use dioxus::prelude::*;
use tantrix_game::Move;

use crate::components::game_actions::handle_rotation_select;
use crate::components::tile::HexSvg;
use crate::state::game_state::GameContext;
use crate::state::hooks::use_phase_view;
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
    let ctx = use_context::<GameContext>();
    let game_state = ctx.game_state;
    let ui_state = ctx.ui_state;
    let mut hovered = ui_state.hovered();

    let phase_view = use_phase_view();
    let Some(wheel) = phase_view().wheel else {
        return rsx! {};
    };

    let tile = wheel.tile;
    let position = wheel.position;

    rsx! {
        div { class: "rotation-bar",
            for rotation in wheel.rotations.iter() {
                {
                    let m = Move { tile, position, rotation: *rotation, forced: false };
                    let cx = SVG_SIZE / 2.0;
                    let cy = SVG_SIZE / 2.0;
                    let rot = *rotation;

                    rsx! {
                        div {
                            class: "rotation-bar-slot",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                handle_rotation_select(rot, game_state, ui_state);
                            },
                            onmouseenter: move |_| hovered.set(Hovered::Rotation(rot)),
                            onmouseleave: move |_| hovered.set(Hovered::None),
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
