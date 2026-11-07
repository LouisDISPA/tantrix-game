//! Reactive hooks that derive display data from game + UI state.
//!
//! Each sub-module owns a concern:
//!
//! * [`game_ctx`] — builds a game page's `GameCtx` (and runs its AI seats).
//! * [`online_game`] — the online game page's `GameCtx`, fed by room snapshots.
//! * [`camera`] — the board camera's controls (zoom, pan) for the viewport.
//! * [`phase_view`] — the board layers' view of the current turn phase: valid
//!   positions, preview, orientations (one memo each).
//! * [`scoring`] — end-of-game scoring path highlights.
//! * [`derived`] — small memos used across multiple components.
//! * [`keyboard`] — taking the keyboard's focus a key handler asked for.

#[cfg(feature = "android")]
pub(crate) mod android_updates;
pub(crate) mod camera;
pub(crate) mod derived;
#[cfg(feature = "desktop")]
pub(crate) mod desktop_updates;
#[cfg(not(native_client))]
pub(crate) mod downloads;
pub(crate) mod game_ctx;
pub(crate) mod keyboard;
pub(crate) mod online_game;
pub(crate) mod phase_view;
pub(crate) mod scoring;

pub(crate) use camera::use_board_camera;
pub(crate) use derived::{
    use_current_player, use_has_forced_move, use_is_game_over, use_placed_moves,
};
pub(crate) use game_ctx::use_game_ctx;
pub(crate) use keyboard::use_take_focus;
pub(crate) use online_game::use_online_game_ctx;
pub(crate) use phase_view::{use_orientations, use_preview_placement, use_valid_positions};
pub(crate) use scoring::use_highlight_map;
