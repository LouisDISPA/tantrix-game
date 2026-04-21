use dioxus::prelude::*;
use tantrix_game::Position;

use crate::components::tile::{BoardTile, BoardTileVariant, EmptyHex};
use crate::state::actions::click_position;
use crate::state::game_state::GameCtx;
use crate::hooks::use_phase_view;
use crate::state::ui_state::{Hovered, UiStateStoreExt};
use crate::svg::geometry::HEX_SIZE;

/// Renders the click-target empty hexes for the current turn phase, plus the
/// ghost preview of the move being considered.
#[component]
pub fn BoardEmptyLayer() -> Element {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    let phase_view = use_phase_view();
    let view = phase_view();

    rsx! {
        for vp in view.positions.iter() {
            EmptyHex {
                key: "e{vp.pos.x()},{vp.pos.y()}",
                position: vp.pos,
                hex_size: HEX_SIZE,
                forced: vp.forced,
                faint: vp.faint,
                onclick: move |pos: Position| click_position(ctx, pos),
                onmouseenter: move |pos: Position| {
                    ui_state.hovered().set(Hovered::BoardPosition(pos));
                },
                onmouseleave: move |_pos: Position| {
                    ui_state.hovered().set(Hovered::None);
                },
            }
        }
        if let Some(preview) = view.preview {
            BoardTile {
                m: preview,
                hex_size: HEX_SIZE,
                variant: BoardTileVariant::Preview,
            }
        }
    }
}
