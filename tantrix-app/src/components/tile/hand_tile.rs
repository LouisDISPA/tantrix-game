use std::rc::Rc;

use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use tantrix_game::{Color, Rotation};

use crate::components::tile::hex_svg::HexSvg;
use crate::state::actions::click_tile;
use crate::state::game_state::{GameCtx, GameState};
use crate::state::gesture::{Pointer, MOUSE_SLOP, TOUCH_SLOP};
use crate::state::tile_drag::{press_tile, DragPhase};
use crate::state::ui_state::{HandTileState, Hovered, UiStateStoreExt};
use crate::svg::geometry::{Rect, TILE_CX, TILE_CY, TILE_HEX, TILE_VB_H, TILE_VB_W};
use crate::svg::paths::hex_polygon_points;

/// Slightly larger hex used for selection/forced outline rings. It spills
/// outside the tile's tight viewBox, which the stylesheet lets show.
const OUTLINE_HEX: f64 = TILE_HEX + 3.0;

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

    let state = use_memo(move || {
        let gs = ctx.game_state.read();
        // Only the player to move has a selection: a new one leaves the
        // other hands' memos alone.
        let selected = if gs.current_player() == color {
            ctx.ui_state.selected_tile()()
        } else {
            None
        };
        compute_state(&gs, selected, color, index())
    });

    let just_drawn = use_memo(move || ctx.ui_state.new_tile()() == Some((color, index())));

    // Only the current seat's tiles can be played, and only by a player who
    // plays that seat on this device (any seat locally, their own online).
    let is_active_seat = use_memo(move || ctx.game_state.read().current_player() == color);
    let can_play = use_memo(move || is_active_seat() && ctx.driver.controls(color));
    let lifted = use_memo(move || {
        ctx.ui_state.tile_drag()().is_some_and(|drag| {
            drag.color == color && drag.index == index() && drag.phase != DragPhase::Pressed
        })
    });
    let mut mounted = use_signal(|| None::<Rc<MountedData>>);
    let mut bounds = use_signal(|| None::<Rect>);
    let measure = move || {
        spawn(async move {
            let element = mounted.peek().clone();
            if let Some(element) = element {
                if let Ok(r) = element.get_client_rect().await {
                    let origin = Rect::new(r.min_x(), r.min_y(), r.max_x(), r.max_y());
                    bounds.set(Some(origin));
                    // Refresh at press, too: selection and hover can lift a tile.
                    let mut dragging = ctx.ui_state.tile_drag();
                    let drag = *dragging.peek();
                    if let Some(mut drag) = drag {
                        if drag.color == color
                            && drag.index == index()
                            && drag.phase == DragPhase::Pressed
                        {
                            drag.origin = origin;
                            dragging.set(Some(drag));
                        }
                    }
                }
            }
        });
    };

    rsx! {
        if let Some(tile) = tile() {
            {
                let state = state();
                let show_outline = matches!(state, HandTileState::Selected | HandTileState::Forced);
                let outline_class = match state {
                    HandTileState::Selected => "hand-hex-ring selected",
                    _ => "hand-hex-ring forced",
                };
                let outline_points = hex_polygon_points(TILE_CX, TILE_CY, OUTLINE_HEX);
                let state_class = state.css_class();
                let interactive = state.is_interactive() && can_play();

                rsx! {
                    div {
                        class: "hand-tile",
                        class: "{state_class}",
                        class: if just_drawn() { "new-tile" },
                        class: if lifted() { "lifted" },
                        "data-tile": "{tile:?}",
                        "data-draggable": "{interactive}",
                        draggable: false,
                        role: "button",
                        tabindex: if interactive { "0" } else { "-1" },
                        aria_label: "Select tile",
                        onmounted: move |evt| {
                            mounted.set(Some(evt.data()));
                            measure();
                        },
                        onresize: move |_| measure(),
                        ondragstart: move |evt| evt.prevent_default(),
                        onpointerdown: move |evt| {
                            if interactive && evt.data().trigger_button() == Some(MouseButton::Primary) {
                                if let Some(origin) = *bounds.peek() {
                                    let at = evt.data().client_coordinates();
                                    let slop = if evt.data().pointer_type() == "mouse" { MOUSE_SLOP } else { TOUCH_SLOP };
                                    press_tile(ctx, color, index(), Pointer::new(evt.data().pointer_id(), at.x, at.y), origin, slop);
                                    measure();
                                }
                            }
                        },
                        onclick: move |_| {
                            if *ctx.ui_state.suppress_tile_click().peek() {
                                ctx.ui_state.suppress_tile_click().set(false);
                                return;
                            }
                            if interactive {
                                click_tile(ctx, color, index());
                            }
                        },
                        onkeydown: move |e| {
                            if interactive
                                && (matches!(e.key(), Key::Enter)
                                    || matches!(e.key(), Key::Character(c) if c == " "))
                            {
                                click_tile(ctx, color, index());
                            }
                        },
                        // Only a mouse hovers: a finger's "hover" would stick
                        // after the tap.
                        onpointerenter: move |e| {
                            if is_active_seat() && e.data().pointer_type() == "mouse" {
                                hovered.set(Hovered::HandTile(index()));
                            }
                        },
                        onpointerleave: move |e| {
                            if is_active_seat() && e.data().pointer_type() == "mouse" && *hovered.peek() == Hovered::HandTile(index()) {
                                hovered.set(Hovered::None);
                            }
                        },
                        svg { view_box: "0 0 {TILE_VB_W:.1} {TILE_VB_H:.1}",
                            if show_outline {
                                polygon { points: outline_points, class: outline_class }
                            }
                            HexSvg {
                                tile,
                                rotation: Rotation::None,
                                cx: TILE_CX,
                                cy: TILE_CY,
                                hex_size: TILE_HEX,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The visible state of `color`'s hand tile at `index`, `selected` being
/// the tile the player to move has selected. The other players' tiles are
/// always Idle (inert but not dimmed); the player to move's tiles are
/// Selected, Forced (the tile fills a forced space), Disabled (it has no
/// legal move, or another tile must fill a forced space) or Idle.
fn compute_state(
    gs: &GameState,
    selected: Option<usize>,
    color: Color,
    index: usize,
) -> HandTileState {
    if gs.current_player() != color {
        return HandTileState::Idle;
    }
    if selected == Some(index) {
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

#[cfg(test)]
mod tests {
    use tantrix_game::{Color, Tile, TurnStep};

    use super::compute_state;
    use crate::state::game_state::GameState;
    use crate::state::test_games::{find_game, fixture_after, seeded};
    use crate::state::ui_state::HandTileState;

    /// Every tile's state in `color`'s hand.
    fn hand_states(game: &GameState, selected: Option<usize>, color: Color) -> Vec<HandTileState> {
        (0..game.hand(color).len())
            .map(|index| compute_state(game, selected, color, index))
            .collect()
    }

    /// The fixture's game after Red's free move: Red keeps the turn to
    /// fill the forced space it made, with the tile at the index returned.
    fn red_must_fill_a_forced_space() -> (GameState, usize) {
        let game = fixture_after(3);
        assert_eq!(game.current_player(), Color::Red);
        assert_eq!(game.turn_step(), TurnStep::AfterFreeMove);
        let forced = game
            .hand(Color::Red)
            .iter()
            .position(|&t| t == Tile::GYYBGB);
        (game, forced.unwrap())
    }

    #[test]
    fn the_waiting_players_tiles_stay_idle() {
        let (game, _) = red_must_fill_a_forced_space();
        // Inert but not dimmed, even at the index Red has selected.
        for selected in [None, Some(0)] {
            assert!(hand_states(&game, selected, Color::Blue)
                .iter()
                .all(|&state| state == HandTileState::Idle));
        }
    }

    #[test]
    fn the_tile_that_fills_a_forced_space_is_forced_and_the_others_wait() {
        let (game, forced) = red_must_fill_a_forced_space();
        for (index, state) in hand_states(&game, None, Color::Red).into_iter().enumerate() {
            let expected = if index == forced {
                HandTileState::Forced
            } else {
                HandTileState::Disabled
            };
            assert_eq!(state, expected, "tile {index}");
        }
    }

    #[test]
    fn the_selected_tile_shows_selected_over_forced() {
        let (game, forced) = red_must_fill_a_forced_space();
        let states = hand_states(&game, Some(forced), Color::Red);
        assert_eq!(states[forced], HandTileState::Selected);
        assert_eq!(
            states
                .iter()
                .filter(|&&s| s == HandTileState::Disabled)
                .count(),
            states.len() - 1
        );
    }

    #[test]
    fn every_opening_tile_can_be_played() {
        let game = seeded(3);
        let mover = game.current_player();
        assert!(hand_states(&game, None, mover)
            .iter()
            .all(|&state| state == HandTileState::Idle));
        assert_eq!(
            compute_state(&game, Some(2), mover, 2),
            HandTileState::Selected
        );
    }

    #[test]
    fn a_tile_without_a_legal_move_is_disabled() {
        // No forced space, but a tile that fits nowhere next to others that do.
        let (game, stuck) = find_game(|game| {
            let infos = game.hand_info();
            let any_forced = infos.iter().any(|info| info.has_forced);
            let stuck = infos.iter().position(|info| !info.has_moves())?;
            (!any_forced && infos.iter().any(|info| info.has_moves())).then_some(stuck)
        });
        let states = hand_states(&game, None, game.current_player());
        for (index, (state, info)) in states.iter().zip(game.hand_info()).enumerate() {
            let expected = if info.has_moves() {
                HandTileState::Idle
            } else {
                HandTileState::Disabled
            };
            assert_eq!(*state, expected, "tile {index}");
        }
        assert_eq!(states[stuck], HandTileState::Disabled);
        assert!(!HandTileState::Disabled.is_interactive());
    }
}
