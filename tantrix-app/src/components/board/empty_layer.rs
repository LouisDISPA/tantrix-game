use std::rc::Rc;

use dioxus::prelude::*;
use tantrix_game::{Placement, Position};

use crate::components::tile::{spot_id, BoardTile, BoardTileVariant, EmptyHex};
use crate::hooks::phase_view::ValidPosition;
use crate::hooks::{use_current_player, use_pointer_press, use_take_focus};
use crate::state::game_state::GameCtx;
use crate::state::keyboard::{
    choose_spot, focus_spots, is_choice, move_spot, reading_key, FocusTarget, Nav,
};
use crate::state::ui_state::{Hovered, TurnPhase, UiStateStoreExt};
use crate::svg::geometry::{
    hex_position_style, HEX_SIZE, TILE_CX, TILE_CY, TILE_HEX, TILE_VB_H, TILE_VB_W,
};
use crate::svg::paths::hex_polygon_points;

/// Renders the click-target empty hexes for the current turn phase, in the
/// order the keys read them (`state::keyboard`).
///
/// `positions` comes from the board (`use_valid_positions`): this layer
/// re-renders when the set of hexes changes, not when one is hovered.
#[component]
pub fn BoardEmptyLayer(positions: ReadSignal<Vec<ValidPosition>>) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("BoardEmptyLayer", ());
    let spots = use_memo(move || {
        let mut spots = positions();
        spots.sort_by_key(|vp| reading_key(vp.pos));
        spots
    });

    rsx! {
        BoardSpots { positions,
            for (rank, vp) in spots.read().iter().enumerate() {
                EmptyHex {
                    key: "e{vp.pos.x()},{vp.pos.y()}",
                    position: vp.pos,
                    hex_size: HEX_SIZE,
                    forced: vp.forced,
                    faint: vp.faint,
                    picked: vp.picked,
                    rank: rank + 1,
                    count: spots.read().len(),
                }
            }
        }
    }
}

/// The board's spots as one tab stop for the keyboard: a listbox, while a
/// tile is being placed on this device's turn, whose active option — the
/// hovered spot, so the ghost shows it — is its `aria-activedescendant`.
/// The focus stays on this box and never moves onto the hexes: the arrow
/// keys move the active spot (`state::keyboard` reads the board's rows),
/// Enter takes it. A ring marks it while the keyboard has the focus here.
///
/// It reads the hovered spot only while it has the focus, so a mouse
/// hovering the board does not wake it.
#[component]
fn BoardSpots(positions: ReadSignal<Vec<ValidPosition>>, children: Element) -> Element {
    let ctx = use_context::<GameCtx>();
    let ui = ctx.ui_state;
    let mut hovered = ui.hovered();
    let current = use_current_player();
    let press = use_pointer_press();
    let mut focused = use_signal(|| false);
    let mut element = use_signal(|| None::<Rc<MountedData>>);

    // A tile is being placed, on a seat this device plays.
    let takes_keys = use_memo(move || {
        matches!(
            ui.phase()(),
            TurnPhase::SelectPosition | TurnPhase::SelectRotation
        ) && ctx.driver.controls(current())
    });
    use_take_focus(element, move || {
        ui.focus_request()() == Some(FocusTarget::Spots) && takes_keys()
    });
    let active = use_memo(move || {
        if !focused() {
            return None;
        }
        let at = ui.hovered()().board_position()?;
        positions.read().iter().any(|vp| vp.pos == at).then_some(at)
    });

    rsx! {
        div {
            class: "board-spots",
            role: if takes_keys() { "listbox" },
            tabindex: if takes_keys() { "0" } else { "-1" },
            aria_label: "Where to place the tile",
            aria_activedescendant: active().map(spot_id),
            onmounted: move |e| element.set(Some(e.data())),
            onpointerdown: move |_| press.down(),
            onfocus: move |_| {
                focused.set(true);
                if takes_keys() && !press.by_pointer() {
                    focus_spots(ctx);
                }
            },
            onblur: move |_| {
                focused.set(false);
                press.reset();
                if hovered.peek().board_position().is_some() {
                    hovered.set(Hovered::None);
                }
            },
            onkeydown: move |e| {
                if !takes_keys() {
                    return;
                }
                let key = e.key();
                if is_choice(&key) {
                    e.prevent_default();
                    choose_spot(ctx);
                } else if let Some(nav) = Nav::of(&key, e.modifiers()) {
                    e.prevent_default();
                    move_spot(ctx, nav);
                }
            },
            {children}
            if let Some(at) = active() {
                SpotRing { position: at }
            }
        }
    }
}

/// The keyboard's ring: a hex a little larger than a tile, round it. It
/// spills out of the tile's viewBox, which the stylesheet lets show.
pub const RING_HEX: f64 = TILE_HEX + 3.0;

/// The keyboard's ring round the active spot. The stylesheet shows it only
/// while the spots have the keyboard's focus (`:focus-visible`).
#[component]
fn SpotRing(position: Position) -> Element {
    let style = hex_position_style(position, HEX_SIZE);
    let points = hex_polygon_points(TILE_CX, TILE_CY, RING_HEX);
    rsx! {
        div { class: "spot-ring", style, aria_hidden: "true",
            svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                polygon { class: "focus-hex-halo", points: points.clone() }
                polygon { class: "focus-hex-line", points }
            }
        }
    }
}

/// The ghost preview of the move being considered, drawn over the empty
/// hexes. Its own component, so hovering re-renders the ghost alone.
#[component]
pub fn BoardPreview(placement: ReadSignal<Option<Placement>>) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("BoardPreview", ());

    rsx! {
        if let Some(preview) = placement() {
            BoardTile {
                placement: preview,
                hex_size: HEX_SIZE,
                variant: BoardTileVariant::Preview,
            }
        }
    }
}
