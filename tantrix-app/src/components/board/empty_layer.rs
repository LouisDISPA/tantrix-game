use dioxus::prelude::*;
use tantrix_game::Position;

use crate::components::game_actions::handle_position_click;
use crate::components::tile::{BoardTile, BoardTileVariant, EmptyHex};
use crate::state::game_state::GameContext;
use crate::state::hooks::use_phase_view;
use crate::state::ui_state::{Hovered, UiStateStoreExt};
use crate::svg::geometry::HEX_SIZE;

/// Renders the click-target empty hexes for the current turn phase, plus the
/// ghost preview of the move being considered.
#[component]
pub fn BoardEmptyLayer() -> Element {
    let ctx = use_context::<GameContext>();
    let game_state = ctx.game_state;
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
                onclick: move |pos: Position| {
                    handle_position_click(pos, game_state, ui_state);
                },
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
