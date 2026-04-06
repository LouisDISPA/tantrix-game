use std::time::Duration;

use dioxus::prelude::*;
use tantrix_game::{Color, Move, Position, Rotation, ScoringPath};

use crate::components::game_over_overlay::GameOverOverlay;
use crate::components::hex_empty::HexEmpty;
use crate::components::hex_tile::HexTile;
use crate::components::rotation_wheel::RotationWheel;
use crate::components::status_bar::StatusBar;
use crate::components::tile_preview::TilePreview;
use crate::state::game_state::{GameContext, GameState};
use crate::state::ui_state::{TurnPhase, UiState};
use crate::svg::geometry::{axial_to_pixel, board_bounding_box, HEX_SIZE};
use crate::svg::paths::{color_css_class, color_link_path, find_color_pairs};

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
    let all_positions: Vec<Position> = placed_moves.iter().map(|m| m.position).collect();

    // Compute scoring paths for game over overlay.
    let scoring_data: Option<Vec<(Color, ScoringPath)>> = if is_game_over {
        let paths = gs.game.scoring_paths();
        let playing = gs.playing_colors().to_vec();
        Some(playing.iter().map(|&c| (c, paths[c].clone())).collect())
    } else {
        None
    };

    let padding = HEX_SIZE * 3.0;
    let (vb_x, vb_y, vb_w, vb_h) =
        if let Some((min_x, min_y, max_x, max_y)) = board_bounding_box(&all_positions, HEX_SIZE) {
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
                            extra_class: if is_game_over { "dimmed-tile".to_string() } else { String::new() },
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

                    // Scoring path highlight overlay
                    if let Some(ref paths_vec) = scoring_data {
                        g { class: "scoring-overlay",
                            for (color , path) in paths_vec.iter() {
                                {
                                    let show = highlighted_color.is_none() || highlighted_color == Some(*color);
                                    if show && !path.positions.is_empty() {
                                        let color = *color;
                                        rsx! {
                                            for pos in path.positions.iter() {
                                                {
                                                    let pos = *pos;
                                                    // Find the placed move at this position.
                                                    let tile_move = placed_moves.iter().find(|m| m.position == pos);
                                                    if let Some(tile_move) = tile_move {
                                                        let (cx, cy) = axial_to_pixel(pos, HEX_SIZE);
                                                        let pairs = find_color_pairs(tile_move);
                                                        let matching_pair = pairs.iter().find(|(_, _, c)| *c == color);
                                                        rsx! {
                                                            if let Some((d1, d2, _)) = matching_pair {
                                                                {
                                                                    let path_d = color_link_path(cx, cy, *d1, *d2, HEX_SIZE);
                                                                    let curve_class = format!(
                                                                        "scoring-curve scoring-curve-{}",
                                                                        color_css_class(color).strip_prefix("hex-curve-").unwrap_or("red"),
                                                                    );
                                                                    rsx! {
                                                                        path {
                                                                            key: "curve-{color:?}-{pos.x()},{pos.y()}",
                                                                            d: "{path_d}",
                                                                            class: "{curve_class}",
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        rsx! {}
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        rsx! {}
                                    }
                                }
                            }
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
            // Show ghost when hovering a tile that has exactly one possible move.
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

    // --- Phase 3: smooth recenter + mark new tile for animation ---
    {
        let mut ui = ui_state.write();
        ui.animating = true;
        ui.pan_x = 0.0;
        ui.pan_y = 0.0;
        ui.new_tile_index = Some(tile_idx);
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

/// After a successful move, check if AI should play next.
/// Steps through each AI move with delays so the player can see what's happening:
/// 1. "thinking" pause → 2. highlight chosen tile → 3. pause → 4. play it → repeat
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

        // Step 1: Compute AI's chosen move
        let chosen = game_state.write().ai_choose();
        let Some(chosen) = chosen else { break };

        // Step 2: Highlight the chosen tile in the hand
        let tile_idx = game_state.read().tile_index(chosen.tile);
        if let Some(idx) = tile_idx {
            let mut ui = ui_state.write();
            ui.selected_tile = Some(idx);
            ui.phase = TurnPhase::WaitingForAi;
        }

        sleep(Duration::from_millis(500)).await;

        // Step 4: Show the position
        {
            let mut ui = ui_state.write();
            ui.selected_position = Some(chosen.position);
        }
        sleep(Duration::from_millis(400)).await;

        // Step 5: Play the move
        {
            let mut gs = game_state.write();
            let _ = gs.play_move(chosen);
        }
        {
            let mut ui = ui_state.write();
            ui.new_tile_index = tile_idx;
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
        ui.new_tile_index = None;
    } else if has_forced {
        ui.reset_selection();
        ui.message = "Forced move! You must fill a forced space.".to_string();
    } else {
        ui.reset_selection();
    }
}
