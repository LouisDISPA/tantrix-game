use std::collections::BTreeMap;

use dioxus::prelude::*;
use tantrix_game::{Color, Move, Position, Rotation, Tile};

use crate::state::game_state::{GameContext, GameStateStoreExt};
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};

// ---------------------------------------------------------------------------
// use_board_camera — encapsulates camera state + event handlers
// ---------------------------------------------------------------------------

pub struct BoardCamera {
    pub transform: String,
    pub zoom_percent: i32,
    pub viewport_class: String,
}

/// Camera event handlers — holds a `Copy`able `Store` so all methods
/// just copy it out of `self` to satisfy Dioxus's ownership model.
#[derive(Clone, Copy)]
pub struct BoardCameraHandlers {
    ui_state: Store<crate::state::ui_state::UiState>,
}

impl BoardCameraHandlers {
    pub fn zoom_in(self) {
        let ui = self.ui_state;
        let z = *ui.zoom().read();
        ui.zoom().set((z * 1.25).clamp(0.3, 5.0));
    }

    pub fn zoom_out(self) {
        let ui = self.ui_state;
        let z = *ui.zoom().read();
        ui.zoom().set((z / 1.25).clamp(0.3, 5.0));
    }

    pub fn reset_zoom(mut self) {
        let mut ui = self.ui_state.write();
        ui.zoom = 1.0;
        ui.pan_x = 0.0;
        ui.pan_y = 0.0;
    }

    pub fn on_wheel(self, evt: &WheelEvent) {
        let ui = self.ui_state;
        let delta_y = evt.data().delta().strip_units().y;
        let z = *ui.zoom().read();
        let factor = if delta_y < 0.0 { 1.1 } else { 0.9 };
        ui.zoom().set((z * factor).clamp(0.3, 5.0));
    }

    pub fn on_drag_start(mut self, evt: &MouseEvent) {
        let mut ui = self.ui_state.write();
        let coords = evt.data().client_coordinates();
        ui.dragging = true;
        ui.last_drag_x = coords.x;
        ui.last_drag_y = coords.y;
    }

    pub fn on_drag_move(self, evt: &MouseEvent) {
        let mut ui = self.ui_state;
        if *ui.dragging().read() {
            let coords = evt.data().client_coordinates();
            let last_x = *ui.last_drag_x().read();
            let last_y = *ui.last_drag_y().read();
            let z = *ui.zoom().read();
            let dx = coords.x - last_x;
            let dy = coords.y - last_y;
            let mut w = ui.write();
            w.pan_x -= dx / z;
            w.pan_y -= dy / z;
            w.last_drag_x = coords.x;
            w.last_drag_y = coords.y;
        }
    }

    pub fn on_drag_end(self) {
        self.ui_state.dragging().set(false);
    }
}

pub fn use_board_camera() -> (BoardCamera, BoardCameraHandlers) {
    let ctx = use_context::<GameContext>();
    let ui_state = ctx.ui_state;

    let zoom = ui_state.zoom()();
    let pan_x = ui_state.pan_x()();
    let pan_y = ui_state.pan_y()();
    let dragging = ui_state.dragging()();

    let transform = format!(
        "transform: scale({:.3}) translate({:.1}px, {:.1}px);",
        zoom, -pan_x, -pan_y
    );

    let mut viewport_class = String::from("board-viewport");
    if dragging {
        viewport_class.push_str(" dragging");
    }

    let camera = BoardCamera {
        transform,
        zoom_percent: (zoom * 100.0) as i32,
        viewport_class,
    };

    let handlers = BoardCameraHandlers { ui_state };

    (camera, handlers)
}

// ---------------------------------------------------------------------------
// use_highlight_map — scoring highlight memo
// ---------------------------------------------------------------------------

pub fn use_highlight_map() -> Memo<BTreeMap<Position, Vec<Color>>> {
    let ctx = use_context::<GameContext>();
    let ui_state = ctx.ui_state;
    let game_state = ctx.game_state;

    use_memo(move || {
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
    })
}

// ---------------------------------------------------------------------------
// use_visible_positions — visible empty positions memo
// ---------------------------------------------------------------------------

pub fn use_visible_positions() -> Memo<Vec<(Position, bool, bool)>> {
    let ctx = use_context::<GameContext>();
    let ui_state = ctx.ui_state;
    let game_state = ctx.game_state;

    use_memo(move || {
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
    })
}

// ---------------------------------------------------------------------------
// use_preview_move — preview move memo
// ---------------------------------------------------------------------------

pub fn use_preview_move() -> Memo<Option<Move>> {
    let ctx = use_context::<GameContext>();
    let ui_state = ctx.ui_state;
    let game_state = ctx.game_state;

    use_memo(move || {
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
                Some(Move {
                    tile,
                    position,
                    rotation,
                    forced: false,
                })
            }
            TurnPhase::SelectRotation => {
                let tile_idx = selected_tile?;
                let position = selected_position?;
                let tile = *hand.get(tile_idx)?;
                let info = hand_info.get(tile_idx)?;
                let rotations = info.positions.get(&position)?;
                let rotation = hovered_rotation.unwrap_or(*rotations.first()?);
                Some(Move {
                    tile,
                    position,
                    rotation,
                    forced: false,
                })
            }
            _ => None,
        }
    })
}

// ---------------------------------------------------------------------------
// use_wheel_data — rotation wheel data memo
// ---------------------------------------------------------------------------

pub fn use_wheel_data() -> Memo<Option<(Tile, Position, Vec<Rotation>)>> {
    let ctx = use_context::<GameContext>();
    let ui_state = ctx.ui_state;
    let game_state = ctx.game_state;

    use_memo(move || {
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
    })
}
