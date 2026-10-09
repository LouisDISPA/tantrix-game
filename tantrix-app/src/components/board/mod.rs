//! Board — the interactive hex map.
//!
//! Split into focused sub-components so each reads only the state it needs
//! and re-renders independently:
//!
//! * [`BoardViewport`] — zoom/pan host wrapping the tile layers.
//! * [`BoardTilesLayer`] — placed moves (with scoring highlights).
//! * [`BoardEmptyLayer`] — valid empty slots.
//! * [`BoardPreview`] — the ghost of the placement being considered.
//! * [`OrientationPicker`] — the card of options at the spot, shown only
//!   during SelectRotation.
//! * [`ZoomControls`] / [`EndGameButton`] — the camera's and the game's
//!   buttons, which the game screen's menu hosts.
//!
//! The board works out the phase's derived values once — the placed moves,
//! the valid positions, the preview, the orientations — and passes each to the
//! layer that shows it as a `ReadSignal`, so every layer subscribes only to
//! its own value (hovering a hex re-renders the preview, not the hexes or
//! the card). The board itself reads only whether the game is over.
//!
//! Layout: the board is the camera's cell of the game screen (`.board-stage`),
//! and nothing else. The HUD and the hand panel are cards floating over the
//! screen around that cell, and the camera frames the board in the cell, so it
//! never parks a legal (clickable) position underneath one; a zoom or a pan
//! may still run the board under them. Only the card of options pops up on
//! the board, at the spot it asks about.

mod controls;
mod empty_layer;
mod orientation_picker;
mod tiles_layer;
mod viewport;

use std::rc::Rc;

use dioxus::prelude::*;

use crate::hooks::{
    use_board_camera, use_current_player, use_is_game_over, use_orientations, use_placed_moves,
    use_preview_placement, use_valid_positions,
};
use crate::svg::geometry::Rect;
use crate::svg::paths::color_name as color_slug;

pub use controls::{EndGameButton, ZoomControls};
pub use empty_layer::{BoardEmptyLayer, BoardPreview};
pub use orientation_picker::OrientationPicker;
pub use tiles_layer::BoardTilesLayer;
pub use viewport::BoardViewport;

/// The board's box. Its class carries the colour of whoever moves
/// (`--turn-color`: the legal spots, the card), so a change of turn re-renders
/// this wrapper and not the board and its layers, which it only hosts.
#[component]
fn BoardContainer(children: Element) -> Element {
    let turn = use_current_player();

    rsx! {
        div { class: "board-container color-{color_slug(turn())}", {children} }
    }
}

#[component]
pub fn Board() -> Element {
    let camera = use_board_camera();
    let is_game_over = use_is_game_over();
    // One copy of the placed tiles, for the viewport's centering and the
    // tiles layer.
    let placed_moves = use_placed_moves();
    // The phase's view of the board, one memo per layer that shows it.
    let positions = use_valid_positions();
    let preview = use_preview_placement();
    let orientations = use_orientations();

    // The stage's box on the screen: the camera frames the board in it, so
    // it is measured when the stage mounts and again whenever it resizes.
    let mut stage = use_signal(|| None::<Rc<MountedData>>);
    let measure = move || {
        spawn(async move {
            let mounted = stage.peek().clone();
            let Some(mounted) = mounted else { return };
            if let Ok(r) = mounted.get_client_rect().await {
                let rect = Rect::new(r.min_x(), r.min_y(), r.max_x(), r.max_y());
                camera.stage_measured(rect);
            }
        });
    };

    rsx! {
        BoardContainer {
            // Playfield: clips the zoomed/panned map and hosts the card of
            // options (anchored to this box).
            div {
                class: "board-stage",
                onmounted: move |evt| {
                    stage.set(Some(evt.data()));
                    measure();
                },
                onresize: move |_| measure(),
                BoardViewport { placed_moves,
                    BoardTilesLayer { placed_moves, dimmed: is_game_over() }
                    BoardEmptyLayer { positions }
                    BoardPreview { placement: preview }
                }

                OrientationPicker { orientations, placed: placed_moves }
            }
        }
    }
}
