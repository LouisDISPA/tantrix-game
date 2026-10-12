use dioxus::prelude::*;
use tantrix_game::Rotation;

use crate::components::tile::hex_svg::HexSvg;
use crate::state::game_state::GameCtx;
use crate::state::tile_drag::DragPhase;
use crate::state::ui_state::UiStateStoreExt;
use crate::svg::geometry::{TILE_CX, TILE_CY, TILE_HEX, TILE_VB_H, TILE_VB_W};

/// Outside the board's transform and the rack: only this tile follows the pointer.
#[component]
pub fn GameDraggedTile() -> Element {
    #[cfg(test)]
    crate::util::render_count::record("GameDraggedTile", ());
    let ctx = use_context::<GameCtx>();
    let visible = use_memo(move || {
        ctx.ui_state.tile_drag()().filter(|drag| drag.phase != DragPhase::Pressed)
    });

    rsx! {
        if let Some(drag) = visible() {
            {
                let returning = drag.phase == DragPhase::Returning;
                let (center, width, rotation) = if returning {
                    (drag.origin.center(), drag.origin.width(), Rotation::None)
                } else if let Some(target) = drag.target {
                    (target.center, target.width, target.rotation)
                } else {
                    let origin = drag.origin.center();
                    (
                        (
                            origin.0 + drag.pointer.x - drag.start.0,
                            origin.1 + drag.pointer.y - drag.start.1 - 12.0,
                        ),
                        drag.origin.width() * 1.08,
                        Rotation::None,
                    )
                };
                rsx! {
                    div {
                        class: "dragged-tile",
                        class: if drag.target.is_some() { "snapped" },
                        class: if returning { "returning" },
                        style: "left: {center.0}px; top: {center.1}px; width: {width}px;",
                        aria_hidden: true,
                        onanimationend: move |evt| {
                            if evt.data().animation_name() == "tile-return"
                                && ctx
                                    .ui_state
                                    .tile_drag()
                                    .peek()
                                    .is_some_and(|d| d.phase == DragPhase::Returning)
                            {
                                ctx.ui_state.tile_drag().set(None);
                            }
                        },
                        svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                            HexSvg {
                                tile: drag.tile,
                                rotation,
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
