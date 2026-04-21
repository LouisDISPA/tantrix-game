use dioxus::prelude::*;
use tantrix_game::{Move, Position, Rotation, Tile};

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

/// Rotation wheel display data.
#[derive(Debug, Clone, PartialEq)]
pub struct WheelData {
    pub tile: Tile,
    pub position: Position,
    pub rotations: Vec<Rotation>,
}

/// Everything the board layers need for the current turn phase. A single memo
/// replaces three separate hooks so phase matching and hand_info lookups
/// happen once per render.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhaseView {
    /// Empty hex positions to show as click targets (empty in Idle / game over).
    pub positions: Vec<ValidPosition>,
    /// Ghost preview of the move being considered (hover or selection-driven).
    pub preview: Option<Move>,
    /// Rotation wheel data — present only during SelectRotation.
    pub wheel: Option<WheelData>,
}

pub fn use_phase_view() -> Memo<PhaseView> {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    use_memo(move || {
        let phase = ui_state.phase()();
        let hovered = ui_state.hovered()();
        let selected_tile = ui_state.selected_tile()();
        let selected_position = ui_state.selected_position()();

        let gs = ctx.game_state.read();
        let current = gs.turn_tracker.current();
        let hand = &gs.hands[current];

        match phase {
            TurnPhase::SelectTile => {
                let Some(idx) = hovered.hand_tile() else {
                    return PhaseView::default();
                };
                let Some(info) = gs.hand_info.get(idx) else {
                    return PhaseView::default();
                };
                let positions = info
                    .positions
                    .keys()
                    .map(|&pos| ValidPosition {
                        pos,
                        forced: info.has_forced,
                        faint: true,
                    })
                    .collect();

                // Auto-preview when a hovered tile has exactly one valid move.
                let preview = hand.get(idx).and_then(|&tile| {
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
                });

                PhaseView { positions, preview, wheel: None }
            }

            TurnPhase::SelectPosition => {
                let Some(idx) = selected_tile else {
                    return PhaseView::default();
                };
                let Some(info) = gs.hand_info.get(idx) else {
                    return PhaseView::default();
                };
                let positions = info
                    .positions
                    .keys()
                    .map(|&pos| ValidPosition {
                        pos,
                        forced: info.has_forced,
                        faint: false,
                    })
                    .collect();

                let preview_pos = hovered.board_position().or(selected_position);
                let preview = preview_pos.and_then(|position| {
                    let tile = *hand.get(idx)?;
                    let rotations = info.positions.get(&position)?;
                    let rotation = hovered.rotation().unwrap_or(*rotations.first()?);
                    Some(Move { tile, position, rotation, forced: false })
                });

                PhaseView { positions, preview, wheel: None }
            }

            TurnPhase::SelectRotation => {
                let Some(idx) = selected_tile else {
                    return PhaseView::default();
                };
                let Some(info) = gs.hand_info.get(idx) else {
                    return PhaseView::default();
                };
                let positions = info
                    .positions
                    .keys()
                    .map(|&pos| ValidPosition {
                        pos,
                        forced: info.has_forced,
                        faint: false,
                    })
                    .collect();

                let Some(position) = selected_position else {
                    return PhaseView { positions, preview: None, wheel: None };
                };
                let Some(tile) = hand.get(idx).copied() else {
                    return PhaseView { positions, preview: None, wheel: None };
                };
                let Some(rotations) = info.positions.get(&position).cloned() else {
                    return PhaseView { positions, preview: None, wheel: None };
                };

                let rotation = hovered
                    .rotation()
                    .or_else(|| rotations.first().copied())
                    .unwrap_or(Rotation::None);
                let preview = Some(Move { tile, position, rotation, forced: false });
                let wheel = Some(WheelData { tile, position, rotations });

                PhaseView { positions, preview, wheel }
            }

            TurnPhase::WaitingForAi | TurnPhase::GameOver => PhaseView::default(),
        }
    })
}
