use dioxus::prelude::*;

use crate::components::board::Board;
use crate::components::game_actions::advance_turn;
use crate::components::player_hand::{HandPosition, PlayerHand};
use crate::pages::local_game::LocalGameContext;
use crate::state::game_state::GameStateStoreExt;
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};
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
    let ctx = use_context::<LocalGameContext>();
    let gs = ctx.game_state;
    let mut ui = ctx.ui_state;

    // On mount, kick off AI turns if the AI plays first.
    use_hook(|| {
        advance_turn(gs, ui);
    });

    // Subscribe only to phase field — won't re-render on zoom/pan/hover
    let phase = ui.phase()();
    let is_game_over = phase == TurnPhase::GameOver;

    // Memoize layout — player colors never change during a game
    let layout = use_memo(move || {
        let colors = gs.turn_tracker().read().playing_colors().to_vec();
        let count = colors.len();
        let assignments = assign_hand_positions(&colors);
        (count, colors, assignments)
    });
    let (player_count, _colors, hand_assignments) = layout();

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
                    ui.write().go_back();
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
