use dioxus::prelude::*;

use crate::components::board::Board;
use crate::components::player_hand::{HandPosition, PlayerHand};
use crate::route::Route;
use crate::state::config::GAME_CONFIG;
use crate::state::game_state::GameState;
use crate::state::theme::{toggle_theme, THEME};
use crate::state::ui_state::{TurnPhase, UiState};
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
            "180px 1fr 180px",
            "\". board .\" \"left board right\" \". board .\"",
        ),
        3 => (
            "0px 1fr 120px",
            "180px 1fr 180px",
            "\". board .\" \"left board right\" \". bottom .\"",
        ),
        4 => (
            "120px 1fr 120px",
            "180px 1fr 180px",
            "\". top .\" \"left board right\" \". bottom .\"",
        ),
        _ => ("1fr", "1fr", "\"board\""),
    }
}

/// Context struct holding the game's reactive state.
/// Uses Signal fields so child components get fine-grained reactivity.
#[derive(Clone, Copy)]
pub struct GameContext {
    pub game_state: Signal<GameState>,
    pub ui_state: Signal<UiState>,
}

#[component]
pub fn GameScreen() -> Element {
    // All hooks MUST be called unconditionally and in the same order.
    // Read config with peek() to avoid subscribing to future changes.
    let mut game_state = use_signal(|| {
        let config = GAME_CONFIG.peek();
        match config.as_ref() {
            Some(c) => GameState::new(c),
            // Fallback: 2-player game (will redirect below)
            None => GameState::new(&crate::state::config::GameConfig::new(2)),
        }
    });
    let mut ui_state = use_signal(UiState::default);

    // Provide context so all children can access game state without prop drilling.
    use_context_provider(|| GameContext {
        game_state,
        ui_state,
    });

    // Redirect if no config was set (after hooks are established).
    let has_config = GAME_CONFIG.peek().is_some();
    if !has_config {
        let nav = navigator();
        let _ = nav.push(Route::MainMenu {});
        return rsx! { div { "Redirecting..." } };
    }

    // Navigate to game over screen when the phase changes to GameOver.
    let is_game_over = ui_state.read().phase == TurnPhase::GameOver;
    if is_game_over {
        let nav = navigator();
        let _ = nav.push(Route::GameOver {});
        return rsx! { div { "Game over..." } };
    }

    // Derive layout from game state.
    let gs = game_state.read();
    let colors = gs.playing_colors().to_vec();
    let player_count = colors.len();
    let hand_assignments = assign_hand_positions(&colors);
    drop(gs);

    let (grid_rows, grid_cols, grid_areas) = grid_template(player_count);

    rsx! {
        div {
            class: "game-layout",
            style: "display: grid; grid-template-rows: {grid_rows}; grid-template-columns: {grid_cols}; grid-template-areas: {grid_areas}; height: 100vh; width: 100vw; gap: 0;",
            tabindex: "0",
            autofocus: true,
            onkeydown: move |evt: KeyboardEvent| {
                if evt.key() == Key::Escape {
                    ui_state.write().go_back();
                }
            },

            button {
                class: "theme-toggle",
                onclick: move |_| toggle_theme(),
                "{THEME.read().icon()}"
            }

            div {
                style: "grid-area: board; min-width: 0; min-height: 0;",
                Board {}
            }

            for (color, position) in hand_assignments.iter() {
                PlayerHand {
                    key: "{color:?}",
                    color: *color,
                    position: *position,
                }
            }
        }
    }
}
