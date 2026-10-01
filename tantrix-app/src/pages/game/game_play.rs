use dioxus::prelude::*;

use crate::components::board::Board;
use crate::components::player_hand::{HandPosition, PlayerHand};
use crate::state::game_state::GameCtx;
use crate::state::ui_state::UiStateStoreImplExt;
use tantrix_game::Color;

/// Determine hand positions based on player count.
fn assign_hand_positions(colors: &[Color]) -> Vec<(Color, HandPosition)> {
    match colors.len() {
        2 => vec![
            (colors[0], HandPosition::Left),
            (colors[1], HandPosition::Right),
        ],
        3 => vec![
            (colors[0], HandPosition::Left),
            (colors[1], HandPosition::Right),
            (colors[2], HandPosition::Bottom),
        ],
        4 => vec![
            (colors[0], HandPosition::Top),
            (colors[1], HandPosition::Right),
            (colors[2], HandPosition::Bottom),
            (colors[3], HandPosition::Left),
        ],
        _ => vec![],
    }
}

/// The board and every player's hand. Changes no state while rendering: a
/// local game's first turn is handed on by the setup screen's Start Game
/// (`actions::start_game`), an online one by the room.
#[component]
pub fn GamePlay() -> Element {
    #[cfg(test)]
    crate::util::render_count::record("GamePlay", ());
    let ctx = use_context::<GameCtx>();
    let mut ui = ctx.ui_state;

    // Where each hand sits — player colors never change during a game, so
    // this memo keeps its value (and the layout its render) for the game.
    let hands = use_memo(move || assign_hand_positions(ctx.game_state.read().playing_colors()));

    rsx! {
        div {
            class: "game-layout players-{hands.read().len()}",
            tabindex: "0",
            autofocus: true,
            onkeydown: move |evt: KeyboardEvent| {
                if evt.key() == Key::Escape {
                    ui.go_back();
                }
            },

            div { class: "board-slot", Board {} }

            for (color , position) in hands.read().iter().copied() {
                PlayerHand { key: "{color:?}", color, position }
            }
        }
    }
}
