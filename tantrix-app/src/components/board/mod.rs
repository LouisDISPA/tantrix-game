//! Board — the interactive hex map.
//!
//! Split into focused sub-components so each reads only the state it needs
//! and re-renders independently:
//!
//! * [`BoardViewport`] — zoom/pan host wrapping the tile layers.
//! * [`BoardTilesLayer`] — placed moves (with scoring highlights).
//! * [`BoardEmptyLayer`] — valid empty slots.
//! * [`BoardPreview`] — the ghost of the placement being considered.
//! * [`RotationWheel`] — shown only during SelectRotation.
//! * [`ZoomControls`] / [`EndGameButton`] — chrome in a reserved control bar.
//!
//! The board works out the phase's derived values once — the placed moves,
//! the valid positions, the preview, the wheel — and passes each to the
//! layer that shows it as a `ReadSignal`, so every layer subscribes only to
//! its own value (hovering a hex re-renders the preview, not the hexes or
//! the wheel). The board itself reads only whether the game is over.
//!
//! Layout: the board is a two-row column. The top row (`.board-stage`) is the
//! clipped playfield; the bottom row (`.board-control-bar`) holds the controls.
//! The controls sit *beside* the playfield rather than floating over it, so the
//! camera can never park a legal (clickable) position underneath them.

mod controls;
mod empty_layer;
mod rotation_wheel;
mod tiles_layer;
mod viewport;

use dioxus::prelude::*;

use crate::components::status_bar::StatusBar;
use crate::hooks::{
    use_is_game_over, use_placed_moves, use_preview_placement, use_rotation_wheel,
    use_valid_positions,
};

pub use controls::{EndGameButton, ZoomControls};
pub use empty_layer::{BoardEmptyLayer, BoardPreview};
pub use rotation_wheel::RotationWheel;
pub use tiles_layer::BoardTilesLayer;
pub use viewport::BoardViewport;

#[component]
pub fn Board() -> Element {
    let is_game_over = use_is_game_over();
    // One copy of the placed tiles, for the viewport's centering and the
    // tiles layer.
    let placed_moves = use_placed_moves();
    // The phase's view of the board, one memo per layer that shows it.
    let positions = use_valid_positions();
    let preview = use_preview_placement();
    let wheel = use_rotation_wheel();

    rsx! {
        div { class: "board-container",
            // Playfield: clips the zoomed/panned map, hosts the HUD chip and
            // the rotation picker (both anchored to this box).
            div { class: "board-stage",
                StatusBar {}

                BoardViewport { placed_moves,
                    BoardTilesLayer { placed_moves, dimmed: is_game_over() }
                    BoardEmptyLayer { positions }
                    BoardPreview { placement: preview }
                }

                RotationWheel { wheel }
            }

            // Reserved strip below the playfield — never overlaps the map.
            div { class: "board-control-bar",
                if !is_game_over() {
                    EndGameButton {}
                }

                ZoomControls {}
            }
        }
    }
}
