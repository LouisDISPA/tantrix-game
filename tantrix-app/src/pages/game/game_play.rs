use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use tantrix_game::Player;

use super::game_hud::GameHud;
use super::player_hand::PlayerHand;
use super::shown_hand::shown_hand;
use super::tile_drag::GameDraggedTile;
use super::turn_banner::TurnBanner;
use crate::components::board::Board;
use crate::hooks::use_current_player;
use crate::state::game_state::GameCtx;
use crate::state::gesture::Pointer;
use crate::state::tile_drag::{cancel_tile_drag, drop_tile, move_tile};
use crate::state::ui_state::{set_if_changed, UiStateStoreExt, UiStateStoreImplExt};

/// The HUD row, the board and the hand panel. One tree for every screen:
/// the stylesheet lays it out as a column on a phone (HUD, board, panel) and
/// as a board with a side panel where there is room. Changes no state while
/// rendering: a local game's first turn is handed on by the setup screen's
/// Start Game (`actions::start_game`), an online one by the room.
///
/// `children` go in the HUD: the online room's badge.
#[component]
pub fn GamePlay(children: Element) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("GamePlay", ());
    let ctx = use_context::<GameCtx>();
    let mut ui = ctx.ui_state;
    let current = use_current_player();

    // The seats in turn order — player colors never change during a game,
    // so this memo keeps its value (and the screen its render) for the game.
    let seats = use_memo(move || ctx.game_state.read().playing_colors().to_vec());
    // Whose tiles the panel shows. Only the hands it concerns read it.
    let shown = use_memo(move || {
        let gs = ctx.game_state.read();
        shown_hand(
            ui.peeked()(),
            gs.current_player(),
            ctx.driver.my_color(),
            gs.playing_colors(),
            |seat| !matches!(gs.player(seat), Player::Ai(_)),
        )
    });
    // A peek lasts for one turn.
    use_effect(move || {
        current();
        set_if_changed(ui.peeked(), None);
    });

    rsx! {
        div {
            class: "game-layout players-{seats.read().len()}",
            tabindex: "0",
            autofocus: true,
            onpointerdown: move |evt| {
                if ui.tile_drag().peek().is_some_and(|drag| drag.holds_pointer() && drag.pointer.id != evt.data().pointer_id()) {
                    cancel_tile_drag(ctx);
                }
            },
            onpointermove: move |evt| {
                if evt.data().pointer_type() == "mouse" && !evt.data().held_buttons().contains(MouseButton::Primary) {
                    cancel_tile_drag(ctx);
                    return;
                }
                let at = evt.data().client_coordinates();
                move_tile(ctx, Pointer::new(evt.data().pointer_id(), at.x, at.y));
            },
            onpointerup: move |evt| {
                let at = evt.data().client_coordinates();
                drop_tile(ctx, Pointer::new(evt.data().pointer_id(), at.x, at.y));
            },
            onpointercancel: move |_| cancel_tile_drag(ctx),
            onlostpointercapture: move |evt| {
                if ui.tile_drag().peek().is_some_and(|drag| drag.pointer.id == evt.data().pointer_id()) {
                    cancel_tile_drag(ctx);
                }
            },
            onpointerleave: move |_| cancel_tile_drag(ctx),
            onkeydown: move |evt: KeyboardEvent| {
                if evt.key() == Key::Escape {
                    if ui.tile_drag().peek().is_some_and(|drag| drag.holds_pointer()) {
                        cancel_tile_drag(ctx);
                    } else {
                        ui.go_back();
                    }
                }
            },

            GameHud { {children} }

            div { class: "board-slot", TurnBanner {} Board {} }

            section {
                class: "hand-panel",
                role: "tablist",
                aria_label: "Players' tiles",
                for color in seats.read().iter().copied() {
                    PlayerHand { key: "{color:?}", color, shown }
                }
            }
            GameDraggedTile {}
        }
    }
}
