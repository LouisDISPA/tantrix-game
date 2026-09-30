//! Board — the interactive hex map.
//!
//! Split into focused sub-components so each reads only the state it needs
//! and re-renders independently:
//!
//! * [`BoardViewport`] — zoom/pan host wrapping the tile layers.
//! * [`BoardTilesLayer`] — placed moves (with scoring highlights).
//! * [`BoardEmptyLayer`] — valid empty slots + ghost preview.
//! * [`RotationWheel`] — shown only during SelectRotation.
//! * [`ZoomControls`] / [`EndGameButton`] — chrome in a reserved control bar.
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
use crate::hooks::use_is_game_over;

pub use controls::{EndGameButton, ZoomControls};
pub use empty_layer::BoardEmptyLayer;
pub use rotation_wheel::RotationWheel;
pub use tiles_layer::BoardTilesLayer;
pub use viewport::BoardViewport;

#[component]
pub fn Board() -> Element {
    let is_game_over = use_is_game_over()();

    rsx! {
        div { class: "board-container",
            // Playfield: clips the zoomed/panned map, hosts the HUD chip and
            // the rotation picker (both anchored to this box).
            div { class: "board-stage",
                StatusBar {}

                BoardViewport {
                    BoardTilesLayer {}
                    BoardEmptyLayer {}
                }

                RotationWheel {}
            }

            // Reserved strip below the playfield — never overlaps the map.
            div { class: "board-control-bar",
                if !is_game_over {
                    EndGameButton {}
                }

                ZoomControls {}
            }
        }
    }
}
