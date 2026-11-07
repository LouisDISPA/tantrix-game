use std::rc::Rc;

use dioxus::prelude::*;
use tantrix_game::{Placement, Position};

use crate::components::tile::{spot_id, BoardTile, BoardTileVariant, EmptyHex, FocusHex};
use crate::hooks::phase_view::ValidPosition;
use crate::hooks::{use_current_player, use_take_focus};
use crate::state::game_state::GameCtx;
use crate::state::keyboard::{
    choose_spot, focus_spots, is_choice, keys_in_use, move_spot, FocusTarget, Nav,
};
use crate::state::ui_state::{set_if_changed, Hovered, TurnPhase, UiStateStoreExt};
use crate::svg::geometry::{hex_position_style, HEX_SIZE, TILE_HEX};

/// Renders the click-target empty hexes for the current turn phase.
///
/// `positions` comes from the board (`use_valid_positions`), in the order
/// the keys read them, which a screen reader's count of the spots follows:
/// this layer re-renders when the set of hexes changes, not when one is
/// hovered.
#[component]
pub fn BoardEmptyLayer(positions: ReadSignal<Vec<ValidPosition>>) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("BoardEmptyLayer", ());

    rsx! {
        BoardSpots { positions,
            for vp in positions.iter() {
                EmptyHex {
                    key: "e{vp.pos.x()},{vp.pos.y()}",
                    position: vp.pos,
                    hex_size: HEX_SIZE,
                    forced: vp.forced,
                    faint: vp.faint,
                    picked: vp.picked,
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
/// It reads the hovered spot only while the keys work it (focused from the
/// keyboard, or since a key), so a mouse hovering the board — even after a
/// click on a spot, which focuses this box — does not wake it.
#[component]
fn BoardSpots(positions: ReadSignal<Vec<ValidPosition>>, children: Element) -> Element {
    let ctx = use_context::<GameCtx>();
    let ui = ctx.ui_state;
    let mut hovered = ui.hovered();
    let current = use_current_player();
    // The keys work the spots: it has the keyboard's focus.
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
            onfocus: move |_| {
                if takes_keys() && keys_in_use(ctx) {
                    focused.set(true);
                    focus_spots(ctx);
                }
            },
            onblur: move |_| {
                set_if_changed(focused, false);
                if hovered.peek().board_position().is_some() {
                    hovered.set(Hovered::None);
                }
            },
            onkeydown: move |e| {
                if !takes_keys() {
                    return;
                }
                let key = e.key();
                if is_choice(&key, e.modifiers()) {
                    e.prevent_default();
                    // Once: a held key must not run on into the card.
                    if !e.is_auto_repeating() {
                        set_if_changed(focused, true);
                        choose_spot(ctx);
                    }
                } else if let Some(nav) = Nav::of(&key, e.modifiers()) {
                    e.prevent_default();
                    set_if_changed(focused, true);
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

/// The keyboard's ring round the active spot: a hex a little larger than a
/// tile. The stylesheet shows it only while the spots have the keyboard's
/// focus (`:focus-visible`).
#[component]
fn SpotRing(position: Position) -> Element {
    let style = hex_position_style(position, HEX_SIZE);
    rsx! {
        div { class: "spot-ring", style,
            FocusHex { size: TILE_HEX + 3.0 }
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
