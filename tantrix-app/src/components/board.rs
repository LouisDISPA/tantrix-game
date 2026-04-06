use std::collections::BTreeMap;
use std::time::Duration;

use dioxus::prelude::*;
use tantrix_game::{Color, Move, Position, Rotation};

use crate::components::game_over_overlay::GameOverOverlay;
use crate::components::hex_empty::HexEmpty;
use crate::components::hex_tile::HexTile;
use crate::components::rotation_wheel::RotationWheel;
use crate::components::status_bar::StatusBar;
use crate::components::tile_preview::TilePreview;
use crate::state::game_state::{GameContext, GameState};
use crate::state::ui_state::{TurnPhase, UiState};
use crate::svg::geometry::{board_bounding_box, HEX_SIZE};

#[component]
pub fn Board() -> Element {
    let ctx = use_context::<GameContext>();
    let game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;

    let gs = game_state.read();
    let ui = ui_state.read();

    let is_game_over = ui.phase == TurnPhase::GameOver;
    let highlighted_color = ui.hovered_scoring_color.or(ui.highlighted_color);

    let placed_moves: Vec<Move> = gs.board_moves().to_vec();
    let last_placed = ui.last_placed_position;

    // Center the tiles layer on the bounding box of placed tiles.
    let all_positions: Vec<Position> = placed_moves.iter().map(|m| m.position).collect();
    let (center_x, center_y) = if let Some((min_x, min_y, max_x, max_y)) =
        board_bounding_box(&all_positions, HEX_SIZE)
    {
        ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0)
    } else {
        (0.0, 0.0)
    };

    // Build per-tile highlight colors for game over overlay.
    let highlight_map: BTreeMap<Position, Vec<Color>> = if is_game_over {
        let paths = gs.game.scoring_paths();
        let mut map = BTreeMap::<Position, Vec<Color>>::new();
        for &color in gs.playing_colors() {
            let show = highlighted_color.is_none() || highlighted_color == Some(color);
            if show && !paths[color].positions.is_empty() {
                for &pos in &paths[color].positions {
                    map.entry(pos).or_default().push(color);
                }
            }
        }
        map
    } else {
        BTreeMap::new()
    };

    let zoom = ui.zoom;
    let pan_x = ui.pan_x;
    let pan_y = ui.pan_y;
    let animating = ui.animating;

    let transform = format!(
        "transform: scale({:.3}) translate({:.1}px, {:.1}px);",
        zoom, -pan_x, -pan_y
    );

    let visible_positions = get_visible_positions(&gs, &ui);
    let preview_move = build_preview_move(&gs, &ui);

    let show_rotation_wheel = ui.phase == TurnPhase::SelectRotation;
    let wheel_data = if show_rotation_wheel {
        get_rotation_wheel_data(&gs, &ui)
    } else {
        None
    };

    let dragging = ui.dragging;
    let mut viewport_class = String::from("board-viewport");
    if dragging {
        viewport_class.push_str(" dragging");
    }
    if animating {
        viewport_class.push_str(" animating");
    }

    let zoom_percent = (zoom * 100.0) as i32;

    rsx! {
        div { class: "board-container",
            StatusBar {}

            // Board controls
            if !is_game_over {
                div { class: "board-controls",
                    button {
                        class: "board-ctrl-btn end-game-btn",
                        title: "End game",
                        onclick: move |_| {
                            let mut ui = ui_state.write();
                            ui.phase = TurnPhase::GameOver;
                            ui.message = "Game ended early.".to_string();
                        },
                        "End Game"
                    }
                }
            }

            // Zoom controls
            div { class: "zoom-controls",
                button {
                    class: "zoom-btn",
                    title: "Zoom in",
                    onclick: move |_| {
                        let mut ui = ui_state.write();
                        ui.zoom = (ui.zoom * 1.25).clamp(0.3, 5.0);
                    },
                    "+"
                }
                button {
                    class: "zoom-btn zoom-label",
                    title: "Reset zoom",
                    onclick: move |_| {
                        let mut ui = ui_state.write();
                        ui.animating = true;
                        ui.zoom = 1.0;
                        ui.pan_x = 0.0;
                        ui.pan_y = 0.0;
                    },
                    "{zoom_percent}%"
                }
                button {
                    class: "zoom-btn",
                    title: "Zoom out",
                    onclick: move |_| {
                        let mut ui = ui_state.write();
                        ui.zoom = (ui.zoom / 1.25).clamp(0.3, 5.0);
                    },
                    "−"
                }
            }

            // Viewport wrapper: CSS transform handles zoom/pan with transitions.
            div {
                class: "{viewport_class}",
                style: "{transform}",

                onwheel: move |evt| {
                    let delta_y = evt.data().delta().strip_units().y;
                    let mut ui = ui_state.write();
                    ui.animating = false;
                    let factor = if delta_y < 0.0 { 1.1 } else { 0.9 };
                    ui.zoom = (ui.zoom * factor).clamp(0.3, 5.0);
                },

                onmousedown: move |evt| {
                    let mut ui = ui_state.write();
                    let coords = evt.data().client_coordinates();
                    ui.dragging = true;
                    ui.animating = false;
                    ui.last_drag_x = coords.x;
                    ui.last_drag_y = coords.y;
                },

                onmousemove: move |evt| {
                    let is_dragging = ui_state.read().dragging;
                    if is_dragging {
                        let coords = evt.data().client_coordinates();
                        let last_x = ui_state.read().last_drag_x;
                        let last_y = ui_state.read().last_drag_y;
                        let dx = coords.x - last_x;
                        let dy = coords.y - last_y;
                        let mut ui = ui_state.write();
                        ui.pan_x -= dx / zoom;
                        ui.pan_y -= dy / zoom;
                        ui.last_drag_x = coords.x;
                        ui.last_drag_y = coords.y;
                    }
                },

                onmouseup: move |_| {
                    ui_state.write().dragging = false;
                },

                onmouseleave: move |_| {
                    ui_state.write().dragging = false;
                },

                oncontextmenu: move |evt| {
                    evt.prevent_default();
                    ui_state.write().go_back();
                },

                // Tiles layer: centered on bounding box of placed tiles
                div {
                    class: "board-tiles-layer",
                    style: "transform: translate({-center_x:.1}px, {-center_y:.1}px);",

                    for m in placed_moves.iter() {
                        {
                            let colors = highlight_map
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

                    for (pos , forced , faint) in visible_positions.iter() {
                        HexEmpty {
                            key: "e{pos.x()},{pos.y()}",
                            position: *pos,
                            hex_size: HEX_SIZE,
                            forced: *forced,
                            faint: *faint,
                            onclick: move |pos: Position| {
                                handle_position_click(pos, game_state, ui_state);
                            },
                            onmouseenter: move |pos: Position| {
                                ui_state.write().hovered_position = Some(pos);
                            },
                            onmouseleave: move |_pos: Position| {
                                ui_state.write().hovered_position = None;
                            },
                        }
                    }

                    if let Some(preview) = preview_move {
                        TilePreview { m: preview, hex_size: HEX_SIZE }
                    }
                }
            }

            if let Some((tile, position, rotations)) = wheel_data {
                RotationWheel {
                    tile,
                    position,
                    rotations,
                    on_select: move |rotation: Rotation| {
                        handle_rotation_select(rotation, game_state, ui_state);
                    },
                    on_cancel: move |_| {
                        ui_state.write().go_back();
                    },
                }
            }

            if is_game_over {
                GameOverOverlay {}
            }
        }
    }
}


fn get_visible_positions(gs: &GameState, ui: &UiState) -> Vec<(Position, bool, bool)> {
    match ui.phase {
        TurnPhase::SelectTile => {
            if let Some(idx) = ui.hovered_tile {
                if let Some(info) = gs.hand_info.get(idx) {
                    return info
                        .positions
                        .keys()
                        .map(|&pos| (pos, info.has_forced, true))
                        .collect();
                }
            }
            Vec::new()
        }
        TurnPhase::SelectPosition | TurnPhase::SelectRotation => {
            if let Some(idx) = ui.selected_tile {
                if let Some(info) = gs.hand_info.get(idx) {
                    return info
                        .positions
                        .keys()
                        .map(|&pos| (pos, info.has_forced, false))
                        .collect();
                }
            }
            Vec::new()
        }
        TurnPhase::WaitingForAi | TurnPhase::GameOver => Vec::new(),
    }
}

fn build_preview_move(gs: &GameState, ui: &UiState) -> Option<Move> {
    match ui.phase {
        TurnPhase::SelectTile => {
            let idx = ui.hovered_tile?;
            let hand = gs.get_hand(gs.currently_playing());
            let tile = *hand.get(idx)?;
            let info = gs.hand_info.get(idx)?;
            if info.positions.len() == 1 {
                let (&position, rots) = info.positions.iter().next()?;
                if rots.len() == 1 {
                    return Some(Move {
                        tile,
                        position,
                        rotation: rots[0],
                        forced: false,
                    });
                }
            }
            None
        }
        TurnPhase::SelectPosition => {
            let tile_idx = ui.selected_tile?;
            let position = ui.hovered_position.or(ui.selected_position)?;
            let hand = gs.get_hand(gs.currently_playing());
            let tile = *hand.get(tile_idx)?;
            let info = gs.hand_info.get(tile_idx)?;
            let rotations = info.positions.get(&position)?;
            let rotation = ui.selected_rotation.unwrap_or(*rotations.first()?);
            Some(Move {
                tile,
                position,
                rotation,
                forced: false,
            })
        }
        TurnPhase::SelectRotation => {
            let tile_idx = ui.selected_tile?;
            let position = ui.selected_position?;
            let hand = gs.get_hand(gs.currently_playing());
            let tile = *hand.get(tile_idx)?;
            let info = gs.hand_info.get(tile_idx)?;
            let rotations = info.positions.get(&position)?;
            let rotation = ui.hovered_rotation.unwrap_or(*rotations.first()?);
            Some(Move {
                tile,
                position,
                rotation,
                forced: false,
            })
        }
        _ => None,
    }
}

fn get_rotation_wheel_data(
    gs: &GameState,
    ui: &UiState,
) -> Option<(tantrix_game::Tile, Position, Vec<Rotation>)> {
    let tile_idx = ui.selected_tile?;
    let position = ui.selected_position?;
    let hand = gs.get_hand(gs.currently_playing());
    let tile = *hand.get(tile_idx)?;
    let info = gs.hand_info.get(tile_idx)?;
    let rotations = info.positions.get(&position)?.clone();
    Some((tile, position, rotations))
}

/// Execute a move: build it, play it, then advance the turn.
/// All signal reads happen before any writes to avoid borrow conflicts.
pub fn execute_move(
    tile_idx: usize,
    pos: Position,
    rotation: Rotation,
    mut game_state: Signal<GameState>,
    mut ui_state: Signal<UiState>,
) {
    // --- Phase 1: build the Move from a read-only borrow ---
    let (m, player_color) = {
        let gs = game_state.read();
        let color = gs.currently_playing();
        (gs.build_move(tile_idx, pos, rotation), color)
    };
    let m = match m {
        Some(m) => m,
        None => return,
    };

    // --- Phase 2: play the move (single write, then release) ---
    let result = {
        let mut gs = game_state.write();
        gs.play_move(m)
    };
    if let Err(e) = result {
        ui_state.write().message = format!("Error: {}", e);
        return;
    }

    // --- Phase 3: smooth recenter + mark new tile for animation ---
    {
        let mut ui = ui_state.write();
        ui.animating = true;
        ui.pan_x = 0.0;
        ui.pan_y = 0.0;
        ui.new_tile = Some((player_color, tile_idx));
        ui.last_placed_position = Some(m.position);
    }

    // --- Phase 4: advance the turn with clean read→write cycles ---
    advance_turn(game_state, ui_state);
}

fn handle_position_click(
    pos: Position,
    game_state: Signal<GameState>,
    mut ui_state: Signal<UiState>,
) {
    let (phase, tile_idx, rotations) = {
        let gs = game_state.read();
        let ui = ui_state.read();
        let phase = ui.phase;
        let tile_idx = ui.selected_tile;
        let rots = tile_idx
            .and_then(|idx| gs.hand_info.get(idx))
            .and_then(|info| info.positions.get(&pos))
            .cloned();
        (phase, tile_idx, rots)
    };

    if phase != TurnPhase::SelectPosition {
        return;
    }
    let tile_idx = match tile_idx {
        Some(idx) => idx,
        None => return,
    };
    let rotations = match rotations {
        Some(rots) => rots,
        None => return,
    };

    let board_empty = game_state.read().board_moves().is_empty();

    if rotations.len() == 1 || board_empty {
        execute_move(tile_idx, pos, rotations[0], game_state, ui_state);
    } else {
        let mut ui = ui_state.write();
        ui.selected_position = Some(pos);
        ui.phase = TurnPhase::SelectRotation;
    }
}

fn handle_rotation_select(
    rotation: Rotation,
    game_state: Signal<GameState>,
    ui_state: Signal<UiState>,
) {
    let (tile_idx, pos) = {
        let ui = ui_state.read();
        (ui.selected_tile, ui.selected_position)
    };
    let tile_idx = match tile_idx {
        Some(idx) => idx,
        None => return,
    };
    let pos = match pos {
        Some(p) => p,
        None => return,
    };

    execute_move(tile_idx, pos, rotation, game_state, ui_state);
}

/// After a successful move, check if AI should play next.
pub fn advance_turn(game_state: Signal<GameState>, mut ui_state: Signal<UiState>) {
    let needs_ai = {
        let gs = game_state.read();
        !gs.is_game_over()
            && matches!(
                gs.game.get_player(gs.currently_playing()),
                tantrix_game::Player::Ai(_)
            )
    };

    if needs_ai {
        ui_state.write().phase = TurnPhase::WaitingForAi;
        spawn(async move {
            ai_step_loop(game_state, ui_state).await;
            settle_turn(game_state, ui_state);
        });
    } else {
        settle_turn(game_state, ui_state);
    }
}

async fn ai_step_loop(mut game_state: Signal<GameState>, mut ui_state: Signal<UiState>) {
    use crate::util::sleep;

    loop {
        let (is_ai, has_moves) = {
            let gs = game_state.read();
            let is_ai = !gs.is_game_over()
                && matches!(
                    gs.game.get_player(gs.currently_playing()),
                    tantrix_game::Player::Ai(_)
                );
            (is_ai, gs.has_any_moves())
        };
        if !is_ai {
            break;
        }
        if !has_moves {
            game_state.write().skip_turn();
            continue;
        }

        let (chosen, ai_color) = {
            let mut gs = game_state.write();
            let color = gs.currently_playing();
            (gs.ai_choose(), color)
        };
        let Some(chosen) = chosen else { break };

        let tile_idx = game_state.read().tile_index(chosen.tile);
        if let Some(idx) = tile_idx {
            let mut ui = ui_state.write();
            ui.selected_tile = Some(idx);
            ui.phase = TurnPhase::WaitingForAi;
        }

        sleep(Duration::from_millis(500)).await;

        {
            let mut ui = ui_state.write();
            ui.selected_position = Some(chosen.position);
        }
        sleep(Duration::from_millis(400)).await;

        {
            let mut gs = game_state.write();
            let _ = gs.play_move(chosen);
        }
        {
            let mut ui = ui_state.write();
            ui.new_tile = tile_idx.map(|idx| (ai_color, idx));
            ui.last_placed_position = Some(chosen.position);
            ui.selected_tile = None;
            ui.selected_position = None;
            ui.animating = true;
            ui.pan_x = 0.0;
            ui.pan_y = 0.0;
        }

        sleep(Duration::from_millis(300)).await;
    }
}

fn settle_turn(mut game_state: Signal<GameState>, mut ui_state: Signal<UiState>) {
    game_state.write().skip_stuck_players();

    let (is_over, has_forced, has_moves) = {
        let gs = game_state.read();
        (gs.is_game_over(), gs.has_forced_moves(), gs.has_any_moves())
    };

    let mut ui = ui_state.write();
    if is_over || !has_moves {
        ui.phase = TurnPhase::GameOver;
        ui.message = "Game Over!".to_string();
        ui.new_tile = None;
    } else if has_forced {
        ui.reset_selection();
        ui.message = "Forced move! You must fill a forced space.".to_string();
    } else {
        ui.reset_selection();
    }
}
