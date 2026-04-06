use dioxus::prelude::*;

use crate::components::board::{advance_turn, Board};
use crate::components::player_hand::{HandPosition, PlayerHand};
use crate::pages::local_game::LocalGameContext;
use crate::state::game_state::GameContext;
use crate::state::ui_state::TurnPhase;
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

fn grid_template(player_count: usize) -> (&'static str, &'static str, &'static str) {
    match player_count {
        2 => (
            "0px 1fr 0px",
            "140px 1fr 140px",
            "\". board .\" \"left board right\" \". board .\"",
        ),
        3 => (
            "0px 1fr 140px",
            "140px 1fr 140px",
            "\"left board right\" \"left board right\" \"left bottom right\"",
        ),
        4 => (
            "140px 1fr 140px",
            "140px 1fr 140px",
            "\"left top right\" \"left board right\" \"left bottom right\"",
        ),
        _ => ("1fr", "1fr", "\"board\""),
    }
}

#[component]
pub fn GamePlay() -> Element {
    let mut ctx = use_context::<LocalGameContext>();
    let game_ctx = use_context::<GameContext>();
    let ui_state = ctx.ui_state;

    // On mount, kick off AI turns if the AI plays first.
    use_hook(|| {
        advance_turn(game_ctx.game_state, game_ctx.ui_state);
    });

    let is_game_over = ui_state.read().phase == TurnPhase::GameOver;

    // Derive layout from game state.
    let gs = ctx.game_state.read();
    let colors = gs.playing_colors().to_vec();
    let player_count = colors.len();
    let hand_assignments = assign_hand_positions(&colors);
    drop(gs);

    // When game is over, give the board the full viewport (hide hands).
    let (grid_rows, grid_cols, grid_areas) = if is_game_over {
        ("1fr", "1fr", "\"board\"")
    } else {
        grid_template(player_count)
    };

    rsx! {
        div {
            class: "game-layout",
            style: "display: grid; grid-template-rows: {grid_rows}; grid-template-columns: {grid_cols}; grid-template-areas: {grid_areas}; height: 100vh; width: 100vw; gap: 2px; background: var(--border-subtle);",
            tabindex: "0",
            autofocus: true,
            onkeydown: move |evt: KeyboardEvent| {
                if evt.key() == Key::Escape {
                    ctx.ui_state.write().go_back();
                }
            },

            div { style: "grid-area: board; min-width: 0; min-height: 0;", Board {} }

            if !is_game_over {
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
}
