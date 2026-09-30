use dioxus::prelude::*;
use tantrix_game::{Color, Move, Position, Rotation};

use crate::components::tile::hex_svg::HexSvg;
use crate::state::actions::execute_move;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{HandTileState, Hovered, TurnPhase, UiStateStoreExt};
use crate::svg::paths::hex_polygon_points;

/// Fixed viewBox size for hand tiles — the SVG internal coordinate space.
/// Actual rendered size is controlled entirely by CSS.
const TILE_VB: f64 = 56.0;
const TILE_HEX: f64 = 22.0;
/// Slightly larger hex used for selection/forced outline rings.
const OUTLINE_HEX: f64 = 25.0;

/// A tile in a player's hand. Takes only `(color, index)` — every piece of
/// render state (the tile itself, selected/forced/disabled flags, the
/// just-drawn animation hint) is derived from `GameCtx` through memos.
///
/// `index` is reactive: once the bag is empty, playing a tile shifts the
/// ones after it left, and their memos must follow the new index.
#[component]
pub fn HandTile(color: Color, index: ReadSignal<usize>) -> Element {
    let ctx = use_context::<GameCtx>();
    let mut hovered = ctx.ui_state.hovered();

    let tile = use_memo(move || ctx.game_state.read().hand(color).get(index()).copied());

    let state = use_memo(move || compute_state(ctx, color, index()));

    let just_drawn = use_memo(move || ctx.ui_state.new_tile()() == Some((color, index())));

    // Only the player who owns the current seat can commit a move.
    let is_active_seat = use_memo(move || ctx.game_state.read().current_player() == color);
    let can_play = use_memo(move || {
        if !is_active_seat() {
            return false;
        }
        match (*ctx.my_color.read(), ctx.online.read().is_some()) {
            (Some(mine), _) => mine == color, // online, own this seat
            (None, false) => true,            // local game: any current-player tile is live
            (None, true) => false,            // online player without a seat
        }
    });

    rsx! {
        if let Some(tile) = tile() {
            {
                let cx = TILE_VB / 2.0;
                let cy = TILE_VB / 2.0;
                let m = Move {
                    tile,
                    position: Position::CENTER,
                    rotation: Rotation::None,
                    forced: false,
                };
                let state = state();
                let show_outline = matches!(state, HandTileState::Selected | HandTileState::Forced);
                let outline_class = match state {
                    HandTileState::Selected => "hand-hex-ring selected",
                    _ => "hand-hex-ring forced",
                };
                let outline_points = hex_polygon_points(cx, cy, OUTLINE_HEX);
                let state_class = state.css_class();
                let interactive = state.is_interactive() && can_play();

                rsx! {
                    div {
                        class: "hand-tile",
                        class: "{state_class}",
                        class: if just_drawn() { "new-tile" },
                        "data-tile": "{tile:?}",
                        role: "button",
                        tabindex: if interactive { "0" } else { "-1" },
                        aria_label: "Select tile",
                        onclick: move |_| {
                            if interactive {
                                click_tile(ctx, index());
                            }
                        },
                        onkeydown: move |e| {
                            if interactive
                                && (matches!(e.key(), Key::Enter)
                                    || matches!(e.key(), Key::Character(c) if c == " "))
                            {
                                click_tile(ctx, index());
                            }
                        },
                        onmouseenter: move |_| {
                            if is_active_seat() {
                                hovered.set(Hovered::HandTile(index()));
                            }
                        },
                        onmouseleave: move |_| {
                            if is_active_seat() {
                                hovered.set(Hovered::None);
                            }
                        },
                        svg { view_box: "0 0 {TILE_VB:.0} {TILE_VB:.0}",
                            if show_outline {
                                polygon { points: "{outline_points}", class: "{outline_class}" }
                            }
                            HexSvg { m, cx, cy, hex_size: TILE_HEX }
                        }
                    }
                }
            }
        }
    }
}

/// Derive the visible state of this hand tile. Non-current players' tiles are
/// always Idle (inert but not dimmed); the current player's tiles pick among
/// Selected / Forced / Disabled / Idle based on hand_info and selection.
fn compute_state(ctx: GameCtx, color: Color, index: usize) -> HandTileState {
    let gs = ctx.game_state.read();
    let is_current = gs.current_player() == color;
    if !is_current {
        return HandTileState::Idle;
    }
    if ctx.ui_state.selected_tile()() == Some(index) {
        return HandTileState::Selected;
    }
    let info = gs.hand_info().get(index);
    let has_moves = info.map(|i| i.has_moves()).unwrap_or(false);
    let is_forced = info.map(|i| i.has_forced).unwrap_or(false);
    let any_forced = gs.hand_info().iter().any(|i| i.has_forced);
    if is_forced {
        return HandTileState::Forced;
    }
    if !has_moves || (any_forced && !is_forced) {
        return HandTileState::Disabled;
    }
    HandTileState::Idle
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
        let board_empty = gs.board().is_empty();
        gs.hand_info().get(idx).and_then(|info| {
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
