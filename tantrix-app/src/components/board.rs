use dioxus::prelude::*;
use tantrix_game::{Move, Position, Rotation};

use crate::state::game_state::GameContext;
use crate::components::hex_empty::HexEmpty;
use crate::components::hex_tile::HexTile;
use crate::components::rotation_wheel::RotationWheel;
use crate::components::status_bar::StatusBar;
use crate::components::tile_preview::TilePreview;
use crate::state::game_state::GameState;
use crate::state::ui_state::{TurnPhase, UiState};
use crate::svg::geometry::{board_bounding_box, HEX_SIZE};

#[component]
pub fn Board() -> Element {
    let ctx = use_context::<GameContext>();
    let game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;

    let gs = game_state.read();
    let ui = ui_state.read();

    let placed_moves: Vec<Move> = gs.board_moves().to_vec();
    let all_positions: Vec<Position> = placed_moves.iter().map(|m| m.position).collect();

    let padding = HEX_SIZE * 3.0;
    let (vb_x, vb_y, vb_w, vb_h) = if let Some((min_x, min_y, max_x, max_y)) =
        board_bounding_box(&all_positions, HEX_SIZE)
    {
        let w = max_x - min_x + 2.0 * padding;
        let h = max_y - min_y + 2.0 * padding;
        (min_x - padding, min_y - padding, w, h)
    } else {
        (-100.0, -100.0, 200.0, 200.0)
    };

    let zoom = ui.zoom;
    let pan_x = ui.pan_x;
    let pan_y = ui.pan_y;
    let animating = ui.animating;

    // viewBox auto-fits all placed tiles (baseline, no user zoom/pan).
    let view_box = format!("{:.1} {:.1} {:.1} {:.1}", vb_x, vb_y, vb_w, vb_h);

    // User zoom/pan is applied via CSS transform on a wrapper div.
    // This allows CSS transitions for smooth animated recentering.
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
    let mut svg_class = String::from("board-svg");
    if dragging {
        svg_class.push_str(" dragging");
    }
    if animating {
        svg_class.push_str(" animating");
    }

    let zoom_percent = (zoom * 100.0) as i32;

    rsx! {
        div { class: "board-container",
            StatusBar {}

            // Board controls
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
                        ui.animating = true; // smooth reset
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
                class: "{svg_class}",
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

                svg {
                    class: "board-svg-inner",
                    view_box: "{view_box}",
                    xmlns: "http://www.w3.org/2000/svg",

                    for m in placed_moves.iter() {
                        HexTile {
                            key: "{m.position.x()},{m.position.y()}",
                            m: *m,
                            hex_size: HEX_SIZE,
                        }
                    }

                    for (pos, forced, faint) in visible_positions.iter() {
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
                        TilePreview {
                            m: preview,
                            hex_size: HEX_SIZE,
                        }
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
        TurnPhase::GameOver => Vec::new(),
    }
}

fn build_preview_move(gs: &GameState, ui: &UiState) -> Option<Move> {
    match ui.phase {
        TurnPhase::SelectTile => {
            // Show ghost when hovering a tile that has exactly one possible move.
            let idx = ui.hovered_tile?;
            let hand = gs.get_hand(gs.currently_playing());
            let tile = *hand.get(idx)?;
            let info = gs.hand_info.get(idx)?;
            if info.positions.len() == 1 {
                let (&position, rots) = info.positions.iter().next()?;
                if rots.len() == 1 {
                    return Some(Move { tile, position, rotation: rots[0], forced: false });
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
            Some(Move { tile, position, rotation, forced: false })
        }
        TurnPhase::SelectRotation => {
            let tile_idx = ui.selected_tile?;
            let position = ui.selected_position?;
            let hand = gs.get_hand(gs.currently_playing());
            let tile = *hand.get(tile_idx)?;
            let info = gs.hand_info.get(tile_idx)?;
            let rotations = info.positions.get(&position)?;
            let rotation = ui.hovered_rotation.unwrap_or(*rotations.first()?);
            Some(Move { tile, position, rotation, forced: false })
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
    let m = {
        let gs = game_state.read();
        gs.build_move(tile_idx, pos, rotation)
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

    // --- Phase 3: smooth recenter after placing ---
    {
        let mut ui = ui_state.write();
        ui.animating = true;
        ui.pan_x = 0.0;
        ui.pan_y = 0.0;
    }

    // --- Phase 4: advance the turn with clean read→write cycles ---
    advance_turn(game_state, ui_state);
}

fn handle_position_click(
    pos: Position,
    game_state: Signal<GameState>,
    mut ui_state: Signal<UiState>,
) {
    // Read everything we need into locals, then drop the borrows.
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
        // Single valid rotation, or first move (all rotations equivalent) — auto-play.
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
    // Read tile_idx and pos from ui, then drop the borrow.
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

/// After a successful move, determine the next game state:
/// forced move, normal turn, skip players, or game over.
/// Uses a read-then-write pattern — never holds both borrows at once.
fn advance_turn(mut game_state: Signal<GameState>, mut ui_state: Signal<UiState>) {
    // Read all the info we need in one go.
    let (is_over, has_forced, has_moves, num_players) = {
        let gs = game_state.read();
        (
            gs.is_game_over(),
            gs.has_forced_moves(),
            gs.has_any_moves(),
            gs.playing_colors().len(),
        )
    };

    if is_over {
        let mut ui = ui_state.write();
        ui.phase = TurnPhase::GameOver;
        ui.message = "Game Over!".to_string();
        return;
    }

    if has_forced {
        let mut ui = ui_state.write();
        ui.reset_selection();
        ui.message = "Forced move! You must fill a forced space.".to_string();
        return;
    }

    if has_moves {
        ui_state.write().reset_selection();
        return;
    }

    // Current player has no moves — try skipping through all players.
    for _ in 0..num_players {
        // Read current player state.
        let (can_move, color, is_over) = {
            let gs = game_state.read();
            (gs.has_any_moves(), gs.currently_playing(), gs.is_game_over())
        };

        if is_over {
            let mut ui = ui_state.write();
            ui.phase = TurnPhase::GameOver;
            ui.message = "Game Over!".to_string();
            return;
        }

        if can_move {
            ui_state.write().reset_selection();
            return;
        }

        ui_state.write().message = format!("{} has no valid moves, skipping.", color);

        // Single write to advance the turn.
        game_state.write().skip_turn();
    }

    let mut ui = ui_state.write();
    ui.phase = TurnPhase::GameOver;
    ui.message = "Game Over! No player can move.".to_string();
}
