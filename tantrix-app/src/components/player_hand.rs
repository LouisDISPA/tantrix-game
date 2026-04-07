use dioxus::prelude::*;
use tantrix_game::Color;

use crate::components::game_actions::execute_move;
use crate::components::hand_tile::HandTile;
use crate::state::game_state::{GameContext, GameStateStoreExt, TileMoveInfo};
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};
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

    pub fn grid_area(&self) -> &'static str {
        match self {
            HandPosition::Left => "left",
            HandPosition::Right => "right",
            HandPosition::Top => "top",
            HandPosition::Bottom => "bottom",
        }
    }
}

// ---------------------------------------------------------------------------
// Tile state computation
// ---------------------------------------------------------------------------

struct TileRenderState {
    is_selected: bool,
    is_disabled: bool,
    is_forced: bool,
    is_new: bool,
}

fn compute_tile_state(
    i: usize,
    is_current: bool,
    selected_tile: Option<usize>,
    any_forced: bool,
    new_tile_val: Option<(Color, usize)>,
    color: Color,
    hi: &[TileMoveInfo],
) -> TileRenderState {
    let is_selected = is_current && selected_tile == Some(i);
    let is_disabled = is_current && {
        let has_moves = hi.get(i).map(|info| info.has_moves()).unwrap_or(false);
        let is_forced_tile = hi.get(i).map(|info| info.has_forced).unwrap_or(false);
        !has_moves || (any_forced && !is_forced_tile)
    };
    let is_forced = is_current && hi.get(i).map(|info| info.has_forced).unwrap_or(false);
    let is_new = new_tile_val == Some((color, i));
    TileRenderState { is_selected, is_disabled, is_forced, is_new }
}

// ---------------------------------------------------------------------------
// Tile click handler as a custom hook
// ---------------------------------------------------------------------------

fn use_tile_click(is_current: bool) -> EventHandler<usize> {
    let ctx = use_context::<GameContext>();
    let game_state = ctx.game_state;
    let ui_state = ctx.ui_state;

    EventHandler::new(move |idx: usize| {
        if !is_current {
            return;
        }
        handle_tile_click(idx, game_state, ui_state);
    })
}

fn handle_tile_click(
    idx: usize,
    game_state: Store<crate::state::game_state::GameState>,
    mut ui_state: Store<crate::state::ui_state::UiState>,
) {
    // Check if we should deselect or switch first.
    {
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
    }

    // Try auto-play: if only one position (and one rotation or empty board), play immediately.
    let auto = {
        let board_empty = game_state.board().read().is_empty();
        let hi_store = game_state.hand_info();
        let hi = hi_store.read();
        hi.get(idx).and_then(|info| {
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
        execute_move(idx, pos, rot, game_state, ui_state);
    } else {
        ui_state.write().select_tile(idx);
    }
}

// ---------------------------------------------------------------------------
// PlayerHand component
// ---------------------------------------------------------------------------

#[component]
pub fn PlayerHand(color: Color, position: HandPosition) -> Element {
    let ctx = use_context::<GameContext>();
    let game_state = ctx.game_state;
    let ui_state = ctx.ui_state;

    // Subscribe to individual fields for fine-grained reactivity
    let is_current = game_state.turn_tracker().read().current() == color;
    let hand: Vec<_> = game_state.hands().read()[color].iter().copied().collect();
    let color_var = color_css_var(color);

    // Subscribe to hand_info for move validity
    let hand_info = game_state.hand_info();
    let any_forced = is_current && hand_info.read().iter().any(|info| info.has_forced);

    // Subscribe to individual ui fields (not zoom/pan/drag/etc.)
    let selected_tile = ui_state.selected_tile()();
    let new_tile_val = ui_state.new_tile()();

    // Custom hook for tile click handling
    let tile_click = use_tile_click(is_current);

    let pos_class = position.css_class();
    let grid_area = position.grid_area();
    let active_class = if is_current { " active" } else { "" };
    let tiles_class = if position.is_vertical() {
        "hand-tiles vertical"
    } else {
        "hand-tiles horizontal"
    };

    rsx! {
        div {
            class: "player-hand {pos_class}{active_class}",
            style: "grid-area: {grid_area}; --player-color: {color_var}",

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
                    {
                        let hi = hand_info.read();
                        rsx! {
                            for (i , tile) in hand.iter().enumerate() {
                                {
                                    let state = compute_tile_state(
                                        i, is_current, selected_tile, any_forced,
                                        new_tile_val, color, &hi,
                                    );
                                    rsx! {
                                        HandTile {
                                            key: "{tile:?}",
                                            tile: *tile,
                                            index: i,
                                            is_new: state.is_new,
                                            selected: state.is_selected,
                                            disabled: if is_current { state.is_disabled } else { false },
                                            forced: state.is_forced,
                                            onclick: tile_click,
                                            onmouseenter: move |idx: usize| {
                                                if is_current {
                                                    ui_state.hovered_tile().set(Some(idx));
                                                }
                                            },
                                            onmouseleave: move |_idx: usize| {
                                                if is_current {
                                                    ui_state.hovered_tile().set(None);
                                                }
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
    }
}
