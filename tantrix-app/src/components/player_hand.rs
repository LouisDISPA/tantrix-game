use dioxus::prelude::*;
use tantrix_game::Color;

use crate::components::board::execute_move;
use crate::state::game_state::GameContext;
use crate::components::hand_tile::HandTile;
use crate::state::ui_state::TurnPhase;
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

#[component]
pub fn PlayerHand(color: Color, position: HandPosition) -> Element {
    let ctx = use_context::<GameContext>();
    let game_state = ctx.game_state;
    let mut ui_state = ctx.ui_state;

    let gs = game_state.read();
    let ui = ui_state.read();

    let is_current = gs.currently_playing() == color;
    let hand = gs.get_hand(color);
    let color_var = color_css_var(color);
    let any_forced = is_current && gs.has_forced_moves();

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
                    for (i, tile) in hand.iter().enumerate() {
                        {
                            let is_selected = is_current && ui.selected_tile == Some(i);
                            let is_disabled = is_current && {
                                let has_moves = gs.hand_info.get(i)
                                    .map(|info| info.has_moves())
                                    .unwrap_or(false);
                                let is_forced_tile = gs.hand_info.get(i)
                                    .map(|info| info.has_forced)
                                    .unwrap_or(false);
                                !has_moves || (any_forced && !is_forced_tile)
                            };
                            let is_forced = is_current
                                && gs.hand_info.get(i)
                                    .map(|info| info.has_forced)
                                    .unwrap_or(false);

                            rsx! {
                                HandTile {
                                    key: "{i}",
                                    tile: *tile,
                                    index: i,
                                    selected: is_selected,
                                    disabled: if is_current { is_disabled } else { false },
                                    forced: is_forced,
                                    onclick: move |idx: usize| {
                                        if !is_current {
                                            return;
                                        }

                                        // Check if we should deselect or switch first.
                                        {
                                            let ui = ui_state.read();
                                            match ui.phase {
                                                TurnPhase::SelectPosition | TurnPhase::SelectRotation => {
                                                    if ui.selected_tile == Some(idx) {
                                                        drop(ui);
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
                                            let gs = game_state.read();
                                            let board_empty = gs.board_moves().is_empty();
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
                                            execute_move(idx, pos, rot, game_state, ui_state);
                                        } else {
                                            let mut ui = ui_state.write();
                                            ui.selected_tile = Some(idx);
                                            ui.selected_position = None;
                                            ui.selected_rotation = None;
                                            ui.hovered_position = None;
                                            ui.phase = TurnPhase::SelectPosition;
                                            ui.message.clear();
                                        }
                                    },
                                    onmouseenter: move |idx: usize| {
                                        if is_current {
                                            ui_state.write().hovered_tile = Some(idx);
                                        }
                                    },
                                    onmouseleave: move |_idx: usize| {
                                        if is_current {
                                            ui_state.write().hovered_tile = None;
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
