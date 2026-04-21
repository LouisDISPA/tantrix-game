use dioxus::prelude::*;

use crate::components::board::Board;
use crate::components::player_hand::{HandPosition, PlayerHand};
use crate::pages::local_game::LocalGameContext;
use crate::state::actions::advance_turn;
use crate::state::game_state::{GameContext, GameStateStoreExt};
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

#[component]
pub fn GamePlay() -> Element {
    let local_ctx = use_context::<LocalGameContext>();
    let gs = local_ctx.game_state;
    let mut ui = local_ctx.ui_state;
    let game_ctx = GameContext {
        game_state: gs,
        ui_state: ui,
    };

    // On mount, kick off AI turns if the AI plays first.
    use_hook(|| {
        advance_turn(game_ctx);
    });

    // Memoize layout — player colors never change during a game.
    let layout = use_memo(move || {
        let colors = gs.turn_tracker().read().playing_colors().to_vec();
        let count = colors.len();
        let assignments = assign_hand_positions(&colors);
        (count, colors, assignments)
    });
    let (player_count, _colors, hand_assignments) = layout();

    rsx! {
        div {
            class: "game-layout players-{player_count}",
            tabindex: "0",
            autofocus: true,
            onkeydown: move |evt: KeyboardEvent| {
                if evt.key() == Key::Escape {
                    ui.write().go_back();
                }
            },

            div { class: "board-slot", Board {} }

            for (color , position) in hand_assignments.iter() {
                PlayerHand {
                    key: "{color:?}",
                    color: *color,
                    position: *position,
                }
            }
        }
    }
}
