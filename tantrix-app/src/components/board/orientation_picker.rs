use dioxus::prelude::*;
use tantrix_game::{Move, Position};

use crate::components::tile::HexSvg;
use crate::hooks::phase_view::Orientations;
use crate::state::actions::pick_rotation;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{Hovered, UiStateStoreExt};
use crate::svg::camera_math::{Pose, Size};
use crate::svg::card_placement::{place_card_at, size_style};
use crate::svg::geometry::{TILE_CX, TILE_CY, TILE_HEX, TILE_VB_H, TILE_VB_W};

/// The card of options that pops up at the spot being decided, shown only
/// during SelectRotation: the selected tile in each of the ways it fits
/// there. A tile never fits a spot in more than two ways, so the choice is
/// always between two.
///
/// The card sits on whichever side of the spot hides the fewest placed
/// tiles (`svg::card_placement`), measured on the stage with the board's
/// camera applied, so it follows the spot when the board is panned. Until
/// the stage has been measured it sits at the top, centred.
///
/// Pressing an option (or hovering it, with a mouse) previews that
/// orientation on the board by setting `Hovered::Rotation`; lifting plays
/// it. The card itself does not depend on what is hovered, so a press does
/// not re-render it: only the ghost on the board.
#[component]
pub fn OrientationPicker(
    orientations: ReadSignal<Option<Orientations>>,
    placed: ReadSignal<Vec<Move>>,
) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("OrientationPicker", ());
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;
    let mut hovered = ui_state.hovered();

    // Reads the camera only while the card is open (it returns before that
    // otherwise), so panning the board wakes it then and never before.
    let placement = use_memo(move || {
        orientations()?;
        let at = ui_state.selected_position()()?;
        let stage = ui_state.stage()()?;
        let pose = Pose {
            zoom: ui_state.zoom()(),
            pan_x: ui_state.pan_x()(),
            pan_y: ui_state.pan_y()(),
        };
        let tiles: Vec<Position> = placed.read().iter().map(|m| m.position).collect();
        Some(place_card_at(at, &tiles, pose, Size::of(&stage)))
    });

    rsx! {
        if let Some(Orientations { tile, rotations }) = orientations() {
            {
                let (class, style) = match placement() {
                    Some(at) => (format!("surface surface-blur orientation-card {}", at.side.css_class()), at.style()),
                    None => ("surface surface-blur orientation-card unplaced".to_string(), size_style()),
                };

                rsx! {
                    div {
                        class,
                        style,
                        role: "group",
                        aria_label: "Choose how to place the tile",
                        for rot in rotations {
                            button {
                                key: "{rot:?}",
                                class: "button button-secondary orientation-option",
                                "data-rotation": "{rot:?}",
                                aria_label: "Choose rotation",
                                onclick: move |_| pick_rotation(ctx, rot),
                                // Pressed: the ghost shows what lifting does.
                                onpointerdown: move |_| hovered.set(Hovered::Rotation(rot)),
                                onpointerup: move |_| hovered.set(Hovered::None),
                                onpointercancel: move |_| hovered.set(Hovered::None),
                                // A mouse previews on hover; a finger has no hover.
                                onpointerenter: move |e| {
                                    if e.data().pointer_type() == "mouse" {
                                        hovered.set(Hovered::Rotation(rot));
                                    }
                                },
                                onpointerleave: move |_| hovered.set(Hovered::None),
                                svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                                    HexSvg {
                                        tile,
                                        rotation: rot,
                                        cx: TILE_CX,
                                        cy: TILE_CY,
                                        hex_size: TILE_HEX,
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
