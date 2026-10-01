use dioxus::prelude::*;
use tantrix_game::Position;

use crate::state::actions::click_position;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{Hovered, UiStateStoreExt};
use crate::svg::geometry::{hex_position_style, TILE_CX, TILE_CY, TILE_HEX, TILE_VB_H, TILE_VB_W};
use crate::svg::paths::hex_polygon_points;

/// An empty hex slot on the board. Positioned at a board coordinate; click and
/// hover interactions dispatch directly through `GameCtx`.
#[component]
pub fn EmptyHex(position: Position, hex_size: f64, forced: bool, faint: bool) -> Element {
    let ctx = use_context::<GameCtx>();
    let mut hovered = ctx.ui_state.hovered();

    let style = hex_position_style(position, hex_size);
    let points = hex_polygon_points(TILE_CX, TILE_CY, TILE_HEX);

    rsx! {
        div {
            class: "board-empty hex-empty",
            class: if forced { "forced" },
            class: if faint { "faint" },
            style: style,
            "data-position": "{position.x()},{position.y()}",
            role: "button",
            tabindex: "0",
            aria_label: "Place tile",
            onclick: move |_| click_position(ctx, position),
            onkeydown: move |e| {
                if matches!(e.key(), Key::Enter)
                    || matches!(e.key(), Key::Character(c) if c == " ")
                {
                    click_position(ctx, position);
                }
            },
            onmouseenter: move |_| hovered.set(Hovered::BoardPosition(position)),
            onmouseleave: move |_| hovered.set(Hovered::None),
            svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                polygon { points: points }
            }
        }
    }
}
