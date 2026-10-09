use dioxus::prelude::*;
use tantrix_game::Position;

use crate::hooks::use_board_camera;
use crate::state::actions::click_position;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{Hovered, UiStateStoreExt};
use crate::svg::geometry::{hex_position_style, TILE_CX, TILE_CY, TILE_HEX, TILE_VB_H, TILE_VB_W};
use crate::svg::paths::hex_polygon_points;

/// An empty hex slot on the board. Positioned at a board coordinate; click and
/// hover interactions dispatch directly through `GameCtx`.
#[component]
pub fn EmptyHex(
    position: Position,
    hex_size: f64,
    forced: bool,
    faint: bool,
    picked: bool,
) -> Element {
    let ctx = use_context::<GameCtx>();
    let mut hovered = ctx.ui_state.hovered();
    let camera = use_board_camera();

    let style = hex_position_style(position, hex_size);
    let points = hex_polygon_points(TILE_CX, TILE_CY, TILE_HEX);

    rsx! {
        div {
            class: "board-empty hex-empty",
            class: if forced { "forced" },
            class: if faint { "faint" },
            class: if picked { "picked" },
            style: style,
            "data-position": "{position.x()},{position.y()}",
            role: "button",
            tabindex: "0",
            aria_label: "Place tile",
            onclick: move |evt| {
                // A tap on a spot is not a tap on the board behind it.
                evt.stop_propagation();
                // The lift that ends a drag of the board is not a tap on the hex.
                if !camera.was_drag() {
                    click_position(ctx, position);
                }
            },
            onkeydown: move |e| {
                if matches!(e.key(), Key::Enter)
                    || matches!(e.key(), Key::Character(c) if c == " ")
                {
                    click_position(ctx, position);
                }
            },
            // Only a mouse hovers: a finger's "hover" would stick after the tap.
            onpointerenter: move |e| {
                if e.data().pointer_type() == "mouse" {
                    hovered.set(Hovered::BoardPosition(position));
                }
            },
            onpointerleave: move |e| {
                if e.data().pointer_type() == "mouse" {
                    hovered.set(Hovered::None);
                }
            },
            svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                polygon { points: points }
            }
        }
    }
}
