//! Reactive hooks that derive display data from game + UI state.
//!
//! Each sub-module owns a concern:
//!
//! * [`camera`] — zoom/pan state and event handlers for the board viewport.
//! * [`phase_view`] — the board layers' view of the current turn phase.
//! * [`scoring`] — end-of-game scoring path highlights.
//! * [`derived`] — small memos used across multiple components.

pub mod camera;
pub mod derived;
pub mod phase_view;
pub mod scoring;

pub use camera::use_board_camera;
pub use derived::{use_current_player, use_has_forced_move, use_is_game_over, use_placed_moves};
pub use phase_view::use_phase_view;
pub use scoring::use_highlight_map;
