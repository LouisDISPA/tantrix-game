use dioxus::prelude::*;

use crate::components::tile::HexSvg;
use crate::hooks::phase_view::WheelData;
use crate::state::actions::pick_rotation;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{Hovered, UiStateStoreExt};

const HEX_SIZE: f64 = 28.0;
const SVG_SIZE: f64 = HEX_SIZE * 2.2;

/// The radial menu of candidate rotations shown during SelectRotation phase.
///
/// `wheel` comes from the board (`use_rotation_wheel`) — renders nothing
/// unless the wheel is currently active. Hovering a slot only writes the
/// shared `hovered` field, which moves the board's preview: the wheel itself
/// does not depend on it, so it does not re-render.
#[component]
pub fn RotationWheel(wheel: ReadSignal<Option<WheelData>>) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("RotationWheel", ());
    let ctx = use_context::<GameCtx>();
    let mut hovered = ctx.ui_state.hovered();

    rsx! {
        if let Some(WheelData { tile, rotations }) = wheel() {
            div { class: "rotation-bar",
                for rot in rotations {
                    {
                        let cx = SVG_SIZE / 2.0;
                        let cy = SVG_SIZE / 2.0;

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
}
