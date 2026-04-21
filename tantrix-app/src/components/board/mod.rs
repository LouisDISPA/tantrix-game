//! Board — the interactive hex map.
//!
//! Split into focused sub-components so each reads only the state it needs
//! and re-renders independently:
//!
//! * [`BoardViewport`] — zoom/pan host wrapping the tile layers.
//! * [`BoardTilesLayer`] — placed moves (with scoring highlights).
//! * [`BoardEmptyLayer`] — valid empty slots + ghost preview.
//! * [`RotationWheel`] — shown only during SelectRotation.
//! * [`ZoomControls`] / [`EndGameButton`] — floating chrome.

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
            StatusBar {}

            if !is_game_over {
                EndGameButton {}
            }

            ZoomControls {}

            BoardViewport {
                BoardTilesLayer {}
                BoardEmptyLayer {}
            }

            RotationWheel {}
        }
    }
}
