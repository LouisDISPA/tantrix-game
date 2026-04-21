use dioxus::prelude::*;
use tantrix_game::{Position, Rotation};

use crate::components::game_actions::{handle_position_click, handle_rotation_select};
use crate::components::game_over_overlay::GameOverOverlay;
use crate::components::hex_empty::HexEmpty;
use crate::components::hex_tile::HexTile;
use crate::components::rotation_wheel::RotationWheel;
use crate::components::status_bar::StatusBar;
use crate::components::tile_preview::TilePreview;
use crate::state::game_state::{GameContext, GameStateStoreExt};
use crate::state::hooks::{use_board_camera, use_highlight_map, use_phase_view};
use crate::state::ui_state::{Hovered, TurnPhase, UiStateStoreExt};
use crate::svg::geometry::{board_bounding_box, HEX_SIZE};

#[component]
pub fn Board() -> Element {
    let ctx = use_context::<GameContext>();
    let game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;

    // Custom hooks — all called unconditionally at top level
    let (camera, cam_handlers) = use_board_camera();
    let highlight_map = use_highlight_map();
    let phase_view = use_phase_view();

    // Phase and selection fields
    let phase = ui_state.phase()();
    let is_game_over = phase == TurnPhase::GameOver;
    let last_placed = ui_state.last_placed_position()();

    // Memoize placed moves — only recomputes when board changes
    let placed_moves = use_memo(move || {
        game_state.board()().iter().copied().collect::<Vec<_>>()
    });

    // Memoize center — only recomputes when placed moves change
    let (center_x, center_y) = use_memo(move || {
        let moves = placed_moves();
        let all_positions: Vec<Position> = moves.iter().map(|m| m.position).collect();
        if let Some((min_x, min_y, max_x, max_y)) =
            board_bounding_box(&all_positions, HEX_SIZE)
        {
            ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0)
        } else {
            (0.0, 0.0)
        }
    })();

    rsx! {
        div { class: "board-container",
            StatusBar {}

            if !is_game_over {
                BoardControls {
                    on_end_game: move |_| {
                        ui_state.phase().set(TurnPhase::GameOver);
                        ui_state.message().set("Game ended early.".to_string());
                    },
                }
            }

            ZoomControls {
                zoom_percent: camera.zoom_percent,
                on_zoom_in: move |_| cam_handlers.zoom_in(),
                on_zoom_out: move |_| cam_handlers.zoom_out(),
                on_reset: move |_| cam_handlers.reset_zoom(),
            }

            // Viewport wrapper: CSS transform handles zoom/pan with transitions.
            div {
                class: "{camera.viewport_class}",
                style: "{camera.transform}",

                onwheel: move |evt| cam_handlers.on_wheel(&evt),
                onmousedown: move |evt| cam_handlers.on_drag_start(&evt),
                onmousemove: move |evt| cam_handlers.on_drag_move(&evt),
                onmouseup: move |_| cam_handlers.on_drag_end(),
                onmouseleave: move |_| cam_handlers.on_drag_end(),
                oncontextmenu: move |evt| {
                    evt.prevent_default();
                    ui_state.write().go_back();
                },

                // Tiles layer: centered on bounding box of placed tiles
                div {
                    class: "board-tiles-layer",
                    style: "transform: translate({-center_x:.1}px, {-center_y:.1}px);",

                    {
                        let moves = placed_moves();
                        let hl_map = highlight_map();
                        rsx! {
                            for m in moves.iter() {
                                {
                                    let colors = hl_map
                                        .get(&m.position)
                                        .cloned()
                                        .unwrap_or_default();
                                    let just_placed = last_placed == Some(m.position);
                                    rsx! {
                                        HexTile {
                                            key: "{m.position.x()},{m.position.y()}",
                                            m: *m,
                                            hex_size: HEX_SIZE,
                                            just_placed,
                                            extra_class: if is_game_over { "dimmed-tile".to_string() } else { String::new() },
                                            highlight_colors: colors,
                                        }
                                    }
                                }
                            }
                        }
                    }

                    {
                        let view = phase_view();
                        rsx! {
                            for vp in view.positions.iter() {
                                HexEmpty {
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
                                TilePreview { m: preview, hex_size: HEX_SIZE }
                            }
                        }
                    }
                }
            }

            if let Some(wheel) = phase_view().wheel {
                RotationWheel {
                    tile: wheel.tile,
                    position: wheel.position,
                    rotations: wheel.rotations,
                    on_select: move |rotation: Rotation| {
                        handle_rotation_select(rotation, game_state, ui_state);
                    },
                }
            }

            if is_game_over {
                GameOverOverlay {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Small subcomponents extracted from the Board RSX
// ---------------------------------------------------------------------------

#[component]
fn ZoomControls(
    zoom_percent: i32,
    on_zoom_in: EventHandler,
    on_zoom_out: EventHandler,
    on_reset: EventHandler,
) -> Element {
    rsx! {
        div { class: "zoom-controls",
            button {
                class: "zoom-btn",
                title: "Zoom in",
                onclick: move |_| on_zoom_in.call(()),
                "+"
            }
            button {
                class: "zoom-btn zoom-label",
                title: "Reset zoom",
                onclick: move |_| on_reset.call(()),
                "{zoom_percent}%"
            }
            button {
                class: "zoom-btn",
                title: "Zoom out",
                onclick: move |_| on_zoom_out.call(()),
                "−"
            }
        }
    }
}

#[component]
fn BoardControls(on_end_game: EventHandler) -> Element {
    rsx! {
        div { class: "board-controls",
            button {
                class: "board-ctrl-btn end-game-btn",
                title: "End game",
                onclick: move |_| on_end_game.call(()),
                "End Game"
            }
        }
    }
}
