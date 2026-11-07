use std::rc::Rc;

use dioxus::prelude::*;
use tantrix_game::{Move, Position, Rotation, Tile};

use crate::components::tile::HexSvg;
use crate::hooks::phase_view::Orientations;
use crate::hooks::use_take_focus;
use crate::state::actions::pick_rotation;
use crate::state::game_state::GameCtx;
use crate::state::keyboard::{
    choose_rotation, is_choice, keys_in_use, request_focus, rotation_label, step_in, FocusTarget,
    Nav,
};
use crate::state::ui_state::{set_if_changed, Hovered, UiStateStoreExt};
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
/// Pressing an option (or hovering it, with a mouse, or focusing it from
/// the keyboard) previews that orientation on the board by setting
/// `Hovered::Rotation`; lifting (or Enter) plays it. The card itself does
/// not depend on what is hovered, so a press does not re-render it: only
/// the ghost on the board.
#[component]
pub fn OrientationPicker(
    orientations: ReadSignal<Option<Orientations>>,
    placed: ReadSignal<Vec<Move>>,
) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("OrientationPicker", ());
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

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
                    Some(at) => {
                        (
                            format!(
                                "surface surface-blur orientation-card {}",
                                at.side.css_class(),
                            ),
                            at.style(),
                        )
                    }
                    None => {
                        (
                            "surface surface-blur orientation-card unplaced".to_string(),
                            size_style(),
                        )
                    }
                };
                let count = rotations.len();
                rsx! {
                    div {
                        class,
                        style,
                        role: "group",
                        aria_label: "Choose how to place the tile",
                        for (index, rotation) in rotations.into_iter().enumerate() {
                            OrientationOption {
                                key: "{rotation:?}",
                                tile,
                                rotation,
                                index,
                                count,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One way round on the card: the `index`th of `count`. The keys step
/// along the card (←/→, or R) and take the option in focus (Enter or
/// Space, handled here rather than by the button's own click, so a held
/// key or a Space let go here plays nothing); the first takes the focus
/// when the keyboard opened the card.
#[component]
fn OrientationOption(tile: Tile, rotation: Rotation, index: usize, count: usize) -> Element {
    let ctx = use_context::<GameCtx>();
    let mut hovered = ctx.ui_state.hovered();
    let mut element = use_signal(|| None::<Rc<MountedData>>);
    use_take_focus(element, move || {
        ctx.ui_state.focus_request()() == Some(FocusTarget::Card(index))
    });

    rsx! {
        button {
            class: "button button-secondary orientation-option",
            "data-rotation": "{rotation:?}",
            aria_label: rotation_label(rotation),
            onmounted: move |e| element.set(Some(e.data())),
            onclick: move |_| {
                // A click from the keys (a screen reader's) moves the focus
                // on, as Enter does.
                if keys_in_use(ctx) {
                    choose_rotation(ctx, rotation);
                } else {
                    pick_rotation(ctx, rotation);
                }
            },
            onkeydown: move |e| {
                let key = e.key();
                if is_choice(&key, e.modifiers()) {
                    e.prevent_default();
                    if !e.is_auto_repeating() {
                        choose_rotation(ctx, rotation);
                    }
                } else if let Some(nav) = Nav::of_rotation(&key, e.modifiers()) {
                    e.prevent_default();
                    if let Some(next) = step_in(count, Some(index), nav) {
                        request_focus(ctx, FocusTarget::Card(next));
                    }
                }
            },
            // A Space pressed elsewhere (on the spots) must not click this.
            onkeyup: move |e| {
                if is_choice(&e.key(), e.modifiers()) {
                    e.prevent_default();
                }
            },
            // Pressed: the ghost shows what lifting does.
            onpointerdown: move |_| hovered.set(Hovered::Rotation(rotation)),
            onpointerup: move |_| hovered.set(Hovered::None),
            onpointercancel: move |_| hovered.set(Hovered::None),
            // A mouse previews on hover; a finger has no hover.
            onpointerenter: move |e| {
                if e.data().pointer_type() == "mouse" {
                    hovered.set(Hovered::Rotation(rotation));
                }
            },
            onpointerleave: move |_| hovered.set(Hovered::None),
            // So does the keyboard's focus.
            onfocus: move |_| {
                if keys_in_use(ctx) {
                    set_if_changed(hovered, Hovered::Rotation(rotation));
                }
            },
            onblur: move |_| {
                if *hovered.peek() == Hovered::Rotation(rotation) {
                    hovered.set(Hovered::None);
                }
            },
            svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                HexSvg {
                    tile,
                    rotation,
                    cx: TILE_CX,
                    cy: TILE_CY,
                    hex_size: TILE_HEX,
                }
            }
        }
    }
}
