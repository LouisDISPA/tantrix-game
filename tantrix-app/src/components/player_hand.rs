use dioxus::prelude::*;
use tantrix_game::Color;

use crate::components::tile::{HandTile, HandTileAction};
use crate::state::actions::execute_move;
use crate::state::game_state::{GameCtx, TileMoveInfo};
use crate::state::ui_state::{HandTileState, Hovered, TurnPhase, UiStateStoreExt};
use crate::svg::paths::color_css_var;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandPosition {
    Left,
    Right,
    Top,
    Bottom,
}

impl HandPosition {
    pub fn is_vertical(&self) -> bool {
        matches!(self, HandPosition::Left | HandPosition::Right)
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            HandPosition::Left => "side left-hand",
            HandPosition::Right => "side right-hand right",
            HandPosition::Top => "edge top-hand",
            HandPosition::Bottom => "edge bottom-hand bottom",
        }
    }

    /// CSS class encoding the grid area (paired with `.player-hand.area-*`).
    pub fn area_class(&self) -> &'static str {
        match self {
            HandPosition::Left => "area-left",
            HandPosition::Right => "area-right",
            HandPosition::Top => "area-top",
            HandPosition::Bottom => "area-bottom",
        }
    }
}

// ---------------------------------------------------------------------------
// Tile state computation
// ---------------------------------------------------------------------------

/// Derive the visible state of a single hand tile. Returns Idle for non-current
/// players (their tiles aren't visually dimmed — they're just inert).
fn compute_tile_state(
    i: usize,
    is_current: bool,
    selected_tile: Option<usize>,
    any_forced: bool,
    hi: &[TileMoveInfo],
) -> HandTileState {
    if !is_current {
        return HandTileState::Idle;
    }
    if selected_tile == Some(i) {
        return HandTileState::Selected;
    }
    let has_moves = hi.get(i).map(|info| info.has_moves()).unwrap_or(false);
    let is_forced_tile = hi.get(i).map(|info| info.has_forced).unwrap_or(false);
    if is_forced_tile {
        return HandTileState::Forced;
    }
    if !has_moves || (any_forced && !is_forced_tile) {
        return HandTileState::Disabled;
    }
    HandTileState::Idle
}

// ---------------------------------------------------------------------------
// Tile click handler as a custom hook
// ---------------------------------------------------------------------------

/// Dispatch a hand-tile action. Hover updates happen for the current player
/// only — tiles in other hands raise no-ops. Clicks route through
/// [`click_tile`] which handles selection, deselection, and auto-play.
fn handle_tile_action(action: HandTileAction, ctx: GameCtx, is_current: bool) {
    match action {
        HandTileAction::Click(idx) => {
            if is_current {
                click_tile(ctx, idx);
            }
        }
        HandTileAction::HoverEnter(idx) => {
            if is_current {
                ctx.ui_state.hovered().set(Hovered::HandTile(idx));
            }
        }
        HandTileAction::HoverLeave => {
            if is_current {
                ctx.ui_state.hovered().set(Hovered::None);
            }
        }
    }
}

/// Clicking a hand tile either deselects it (if it's the currently selected
/// one), auto-plays it when only one legal move exists, or selects it so the
/// player can choose a position.
fn click_tile(ctx: GameCtx, idx: usize) {
    let mut ui_state = ctx.ui_state;

    let phase = *ui_state.phase().read();
    match phase {
        TurnPhase::SelectPosition | TurnPhase::SelectRotation => {
            if *ui_state.selected_tile().read() == Some(idx) {
                ui_state.write().reset_selection();
                return;
            }
        }
        TurnPhase::SelectTile => {}
        _ => return,
    }

    let auto = {
        let gs = ctx.game_state.read();
        let board_empty = gs.board.is_empty();
        gs.hand_info.get(idx).and_then(|info| {
            if info.positions.len() == 1 {
                let (&pos, rots) = info.positions.iter().next().unwrap();
                if rots.len() == 1 || board_empty {
                    Some((pos, rots[0]))
                } else {
                    None
                }
            } else {
                None
            }
        })
    };
    if let Some((pos, rot)) = auto {
        execute_move(ctx, idx, pos, rot);
    } else {
        ui_state.write().select_tile(idx);
    }
}

// ---------------------------------------------------------------------------
// PlayerHand component
// ---------------------------------------------------------------------------

#[component]
pub fn PlayerHand(color: Color, position: HandPosition) -> Element {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    let gs = ctx.game_state.read();
    let is_current = gs.turn_tracker.current() == color;
    let hand: Vec<_> = gs.hands[color].iter().copied().collect();
    let hand_info = gs.hand_info.clone();
    let any_forced = is_current && hand_info.iter().any(|info| info.has_forced);
    drop(gs);

    let color_var = color_css_var(color);

    let selected_tile = ui_state.selected_tile()();
    let new_tile_val = ui_state.new_tile()();

    let pos_class = position.css_class();
    let area_class = position.area_class();
    let tiles_class = if position.is_vertical() {
        "hand-tiles vertical"
    } else {
        "hand-tiles horizontal"
    };

    rsx! {
        div {
            class: "player-hand {pos_class} {area_class}",
            class: if is_current { "active" },
            style: "--player-color: {color_var}",

            div { class: "hand-header",
                if is_current {
                    span { class: "active-dot" }
                }
                "{color}"
            }

            if hand.is_empty() {
                div { class: "hand-empty", "No tiles" }
            } else {
                div { class: "{tiles_class}",
                    for (i , tile) in hand.iter().enumerate() {
                        {
                            let state = compute_tile_state(
                                i, is_current, selected_tile, any_forced, &hand_info,
                            );
                            let just_drawn = new_tile_val == Some((color, i));
                            rsx! {
                                HandTile {
                                    key: "{color:?}-{i}",
                                    tile: *tile,
                                    index: i,
                                    state,
                                    just_drawn,
                                    on_action: move |action: HandTileAction| {
                                        handle_tile_action(action, ctx, is_current);
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
