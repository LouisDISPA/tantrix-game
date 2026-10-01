//! The board layers' view of the current turn phase, as three separate
//! memos: the empty hexes to click, the ghost preview, and the rotation
//! wheel.
//!
//! `Board` creates all three once and hands each to the one component that
//! shows it as a `ReadSignal` prop, so each reader re-renders only when its
//! own value changes: hovering a hex moves the preview, not the hex list or
//! the wheel. Each memo also reads only the `UiState` fields its phase
//! needs — the wheel never reads `hovered` at all.

use dioxus::prelude::*;
use tantrix_game::{Placement, Position, Rotation, Tile};

use crate::state::game_state::GameCtx;
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};

/// An empty hex shown on the board as a click target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidPosition {
    pub pos: Position,
    pub forced: bool,
    /// True when the user is only *peeking* (hovered, not selected) — styled faintly.
    pub faint: bool,
}

/// Rotation wheel display data: the selected tile and the rotations that
/// fit at the selected position.
#[derive(Debug, Clone, PartialEq)]
pub struct WheelData {
    pub tile: Tile,
    pub rotations: Vec<Rotation>,
}

/// Empty hex positions to show as click targets: the hovered hand tile's
/// (faintly) while a tile is being chosen, then the selected tile's. Empty
/// while the AI plays and once the game is over.
pub fn use_valid_positions() -> Memo<Vec<ValidPosition>> {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    use_memo(move || {
        let (idx, faint) = match ui_state.phase()() {
            TurnPhase::SelectTile => (ui_state.hovered()().hand_tile(), true),
            TurnPhase::SelectPosition | TurnPhase::SelectRotation => {
                (ui_state.selected_tile()(), false)
            }
            TurnPhase::WaitingForAi | TurnPhase::GameOver => return Vec::new(),
        };
        let Some(idx) = idx else {
            return Vec::new();
        };
        let gs = ctx.game_state.read();
        let Some(info) = gs.hand_info().get(idx) else {
            return Vec::new();
        };
        info.positions
            .keys()
            .map(|&pos| ValidPosition {
                pos,
                forced: info.has_forced,
                faint,
            })
            .collect()
    })
}

/// Ghost preview of the placement being considered: the hovered hand tile's
/// only move, the selected tile at the hovered (or selected) position, or at
/// the selected position in the hovered (or first) rotation.
pub fn use_preview_placement() -> Memo<Option<Placement>> {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    use_memo(move || match ui_state.phase()() {
        TurnPhase::SelectTile => {
            // Auto-preview when a hovered tile has exactly one valid move.
            let idx = ui_state.hovered()().hand_tile()?;
            let gs = ctx.game_state.read();
            let info = gs.hand_info().get(idx)?;
            let tile = *gs.hand(gs.current_player()).get(idx)?;
            if info.positions.len() != 1 {
                return None;
            }
            let (&position, rotations) = info.positions.iter().next()?;
            (rotations.len() == 1).then(|| Placement {
                tile,
                position,
                rotation: rotations[0],
            })
        }

        TurnPhase::SelectPosition => {
            let idx = ui_state.selected_tile()()?;
            let hovered = ui_state.hovered()();
            let position = hovered
                .board_position()
                .or_else(|| ui_state.selected_position()())?;
            let gs = ctx.game_state.read();
            let tile = *gs.hand(gs.current_player()).get(idx)?;
            let rotations = gs.hand_info().get(idx)?.positions.get(&position)?;
            let rotation = hovered.rotation().unwrap_or(*rotations.first()?);
            Some(Placement {
                tile,
                position,
                rotation,
            })
        }

        TurnPhase::SelectRotation => {
            let idx = ui_state.selected_tile()()?;
            let position = ui_state.selected_position()()?;
            let gs = ctx.game_state.read();
            let tile = *gs.hand(gs.current_player()).get(idx)?;
            let rotations = gs.hand_info().get(idx)?.positions.get(&position)?;
            let rotation = ui_state.hovered()()
                .rotation()
                .or_else(|| rotations.first().copied())
                .unwrap_or(Rotation::None);
            Some(Placement {
                tile,
                position,
                rotation,
            })
        }

        TurnPhase::WaitingForAi | TurnPhase::GameOver => None,
    })
}

/// The rotation wheel's tile and the rotations that fit at the selected
/// position — only during SelectRotation. Does not depend on what is
/// hovered: hovering a rotation changes the preview, not the wheel.
pub fn use_rotation_wheel() -> Memo<Option<WheelData>> {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    use_memo(move || {
        if ui_state.phase()() != TurnPhase::SelectRotation {
            return None;
        }
        let idx = ui_state.selected_tile()()?;
        let position = ui_state.selected_position()()?;
        let gs = ctx.game_state.read();
        let tile = *gs.hand(gs.current_player()).get(idx)?;
        let rotations = gs.hand_info().get(idx)?.positions.get(&position)?.clone();
        Some(WheelData { tile, rotations })
    })
}
