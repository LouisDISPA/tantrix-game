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
use crate::state::game_state::{GameContext, GameState, GameStateStoreExt};
use crate::state::ui_state::{TurnPhase, UiState, UiStateStoreExt};
use crate::svg::geometry::{board_bounding_box, HEX_SIZE};

#[component]
pub fn Board() -> Element {
    let ctx = use_context::<GameContext>();
    let game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;

    // --- Individual field subscriptions (fine-grained reactivity) ---

    // Camera fields — only Board viewport cares about these
    let zoom = ui_state.zoom()();
    let pan_x = ui_state.pan_x()();
    let pan_y = ui_state.pan_y()();
    let animating = ui_state.animating()();
    let dragging = ui_state.dragging()();

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

    // Memoize highlight map — only recomputes when board or highlight state changes
    let highlight_map = use_memo(move || {
        let phase = ui_state.phase()();
        if phase != TurnPhase::GameOver {
            return BTreeMap::new();
        }
        let hovered_sc = ui_state.hovered_scoring_color()();
        let highlighted_c = ui_state.highlighted_color()();
        let hl = hovered_sc.or(highlighted_c);
        let board = game_state.board();
        let b = board.read();
        let paths = b.scoring_paths();
        let colors = game_state.turn_tracker().read().playing_colors().to_vec();
        let mut map = BTreeMap::<Position, Vec<Color>>::new();
        for color in &colors {
            let show = hl.is_none() || hl == Some(*color);
            if show && !paths[*color].positions.is_empty() {
                for &pos in &paths[*color].positions {
                    map.entry(pos).or_default().push(*color);
                }
            }
        }
        map
    });

    let transform = format!(
        "transform: scale({:.3}) translate({:.1}px, {:.1}px);",
        zoom, -pan_x, -pan_y
    );

    // Memoize visible positions — read specific fields so field-level writes trigger recompute
    let visible_positions = use_memo(move || {
        let phase = ui_state.phase()();
        let hovered_tile = ui_state.hovered_tile()();
        let selected_tile = ui_state.selected_tile()();
        let hi_store = game_state.hand_info();
        let hand_info = hi_store.read();
        match phase {
            TurnPhase::SelectTile => {
                if let Some(idx) = hovered_tile {
                    if let Some(info) = hand_info.get(idx) {
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
                if let Some(idx) = selected_tile {
                    if let Some(info) = hand_info.get(idx) {
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
    });

    // Memoize preview move — read specific fields for fine-grained reactivity
    let preview_move = use_memo(move || {
        let phase = ui_state.phase()();
        let hovered_tile = ui_state.hovered_tile()();
        let selected_tile = ui_state.selected_tile()();
        let hovered_position = ui_state.hovered_position()();
        let selected_position = ui_state.selected_position()();
        let hovered_rotation = ui_state.hovered_rotation()();
        let hi_store = game_state.hand_info();
        let hand_info = hi_store.read();
        let current = game_state.turn_tracker().read().current();
        let hands = game_state.hands();
        let h = hands.read();
        let hand = &h[current];
        match phase {
            TurnPhase::SelectTile => {
                let idx = hovered_tile?;
                let tile = *hand.get(idx)?;
                let info = hand_info.get(idx)?;
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
                let tile_idx = selected_tile?;
                let position = hovered_position.or(selected_position)?;
                let tile = *hand.get(tile_idx)?;
                let info = hand_info.get(tile_idx)?;
                let rotations = info.positions.get(&position)?;
                let rotation = hovered_rotation.unwrap_or(*rotations.first()?);
                Some(Move { tile, position, rotation, forced: false })
            }
            TurnPhase::SelectRotation => {
                let tile_idx = selected_tile?;
                let position = selected_position?;
                let tile = *hand.get(tile_idx)?;
                let info = hand_info.get(tile_idx)?;
                let rotations = info.positions.get(&position)?;
                let rotation = hovered_rotation.unwrap_or(*rotations.first()?);
                Some(Move { tile, position, rotation, forced: false })
            }
            _ => None,
        }
    });

    let wheel_data = use_memo(move || {
        let phase = ui_state.phase()();
        if phase != TurnPhase::SelectRotation {
            return None;
        }
        let selected_tile = ui_state.selected_tile()();
        let selected_position = ui_state.selected_position()();
        let tile_idx = selected_tile?;
        let position = selected_position?;
        let current = game_state.turn_tracker().read().current();
        let hands = game_state.hands();
        let h = hands.read();
        let tile = *h[current].get(tile_idx)?;
        let hi_store = game_state.hand_info();
        let hand_info = hi_store.read();
        let info = hand_info.get(tile_idx)?;
        let rotations = info.positions.get(&position)?.clone();
        Some((tile, position, rotations))
    });

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
                            ui_state.phase().set(TurnPhase::GameOver);
                            ui_state.message().set("Game ended early.".to_string());
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
                        let z = *ui_state.zoom().read();
                        ui_state.zoom().set((z * 1.25).clamp(0.3, 5.0));
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
                        let z = *ui_state.zoom().read();
                        ui_state.zoom().set((z / 1.25).clamp(0.3, 5.0));
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
                    ui_state.animating().set(false);
                    let z = *ui_state.zoom().read();
                    let factor = if delta_y < 0.0 { 1.1 } else { 0.9 };
                    ui_state.zoom().set((z * factor).clamp(0.3, 5.0));
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
                    if *ui_state.dragging().read() {
                        let coords = evt.data().client_coordinates();
                        let last_x = *ui_state.last_drag_x().read();
                        let last_y = *ui_state.last_drag_y().read();
                        let z = *ui_state.zoom().read();
                        let dx = coords.x - last_x;
                        let dy = coords.y - last_y;
                        let mut w = ui_state.write();
                        w.pan_x -= dx / z;
                        w.pan_y -= dy / z;
                        w.last_drag_x = coords.x;
                        w.last_drag_y = coords.y;
                    }
                },

                onmouseup: move |_| {
                    ui_state.dragging().set(false);
                },

                onmouseleave: move |_| {
                    ui_state.dragging().set(false);
                },

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
                        let vis_pos = visible_positions();
                        rsx! {
                            for (pos , forced , faint) in vis_pos.iter() {
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
                                        ui_state.hovered_position().set(Some(pos));
                                    },
                                    onmouseleave: move |_pos: Position| {
                                        ui_state.hovered_position().set(None);
                                    },
                                }
                            }
                        }
                    }

                    if let Some(preview) = preview_move() {
                        TilePreview { m: preview, hex_size: HEX_SIZE }
                    }
                }
            }

            if let Some((tile, position, rotations)) = wheel_data() {
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


/// Execute a move: build it, play it, then advance the turn.
/// All signal reads happen before any writes to avoid borrow conflicts.
pub fn execute_move(
    tile_idx: usize,
    pos: Position,
    rotation: Rotation,
    mut game_state: Store<GameState>,
    mut ui_state: Store<UiState>,
) {
    // --- Phase 1: build the Move from field-level reads ---
    let player_color = game_state.turn_tracker().read().current();
    let tile = {
        let hands = game_state.hands();
        let h = hands.read();
        match h[player_color].get(tile_idx) {
            Some(t) => *t,
            None => return,
        }
    };
    let forced = game_state
        .hand_info()
        .read()
        .get(tile_idx)
        .map(|info| info.has_forced)
        .unwrap_or(false);
    let m = Move {
        tile,
        position: pos,
        rotation,
        forced,
    };

    // --- Phase 2: play the move (single write, then release) ---
    let result = {
        let mut gs = game_state.write();
        gs.play_move(m)
    };
    if let Err(e) = result {
        ui_state.message().set(format!("Error: {}", e));
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
    game_state: Store<GameState>,
    ui_state: Store<UiState>,
) {
    let (phase, tile_idx, rotations) = {
        let phase = *ui_state.phase().read();
        let tile_idx = *ui_state.selected_tile().read();
        let hi_store = game_state.hand_info();
        let hi = hi_store.read();
        let rots = tile_idx
            .and_then(|idx| hi.get(idx))
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

    let board_empty = game_state.board().read().is_empty();

    if rotations.len() == 1 || board_empty {
        execute_move(tile_idx, pos, rotations[0], game_state, ui_state);
    } else {
        ui_state.selected_position().set(Some(pos));
        ui_state.phase().set(TurnPhase::SelectRotation);
    }
}

fn handle_rotation_select(
    rotation: Rotation,
    game_state: Store<GameState>,
    ui_state: Store<UiState>,
) {
    let (tile_idx, pos) = (
        *ui_state.selected_tile().read(),
        *ui_state.selected_position().read(),
    );
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
pub fn advance_turn(game_state: Store<GameState>, ui_state: Store<UiState>) {
    let needs_ai = {
        let gs = game_state.read();
        !gs.is_game_over()
            && matches!(
                gs.players[gs.turn_tracker.current()],
                tantrix_game::Player::Ai(_)
            )
    };

    if needs_ai {
        ui_state.phase().set(TurnPhase::WaitingForAi);
        spawn(async move {
            ai_step_loop(game_state, ui_state).await;
            settle_turn(game_state, ui_state);
        });
    } else {
        settle_turn(game_state, ui_state);
    }
}

async fn ai_step_loop(mut game_state: Store<GameState>, mut ui_state: Store<UiState>) {
    use crate::util::sleep;

    loop {
        let (is_ai, has_moves) = {
            let gs = game_state.read();
            let is_ai = !gs.is_game_over()
                && matches!(
                    gs.players[gs.turn_tracker.current()],
                    tantrix_game::Player::Ai(_)
                );
            let has_moves = gs.hand_info.iter().any(|info| info.has_moves());
            (is_ai, has_moves)
        };
        if !is_ai {
            break;
        }
        if !has_moves {
            game_state.write().skip_turn();
            continue;
        }

        let (chosen, ai_color) = {
            let gs = game_state.read();
            let color = gs.turn_tracker.current();
            (gs.ai_choose(), color)
        };
        let Some(chosen) = chosen else { break };

        let tile_idx = {
            let hands = game_state.hands();
            let h = hands.read();
            let current = game_state.turn_tracker().read().current();
            h[current].position(&chosen.tile)
        };
        if let Some(idx) = tile_idx {
            ui_state.selected_tile().set(Some(idx));
            ui_state.phase().set(TurnPhase::WaitingForAi);
        }

        sleep(Duration::from_millis(500)).await;

        ui_state.selected_position().set(Some(chosen.position));
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

fn settle_turn(mut game_state: Store<GameState>, mut ui_state: Store<UiState>) {
    game_state.write().skip_stuck_players();

    let (is_over, has_forced, has_moves) = {
        let gs = game_state.read();
        let is_over = gs.is_game_over();
        let has_forced = gs.hand_info.iter().any(|info| info.has_forced);
        let has_moves = gs.hand_info.iter().any(|info| info.has_moves());
        (is_over, has_forced, has_moves)
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
