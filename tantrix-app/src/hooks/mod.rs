//! Reactive hooks that derive display data from game + UI state.
//!
//! Each sub-module owns a concern:
//!
//! * [`game_ctx`] — builds a game page's `GameCtx` (and runs its AI seats).
//! * [`online_game`] — the online game page's `GameCtx`, fed by room snapshots.
//! * [`camera`] — the board camera's controls (zoom, pan) for the viewport.
//! * [`phase_view`] — the board layers' view of the current turn phase: valid
//!   positions, preview, rotation wheel (one memo each).
//! * [`scoring`] — end-of-game scoring path highlights.
//! * [`derived`] — small memos used across multiple components.

pub mod camera;
pub mod derived;
pub mod game_ctx;
pub mod online_game;
pub mod phase_view;
pub mod scoring;

pub use camera::use_board_camera;
pub use derived::{use_current_player, use_has_forced_move, use_is_game_over, use_placed_moves};
pub use game_ctx::use_game_ctx;
pub use online_game::{use_has_snapshot, use_online_game_ctx};
pub use phase_view::{use_preview_placement, use_rotation_wheel, use_valid_positions};
pub use scoring::use_highlight_map;
