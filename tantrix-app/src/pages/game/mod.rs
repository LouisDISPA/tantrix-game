//! The game screens both game pages show: [`GamePlay`] while the game runs,
//! [`GameOver`] once it has ended.
//!
//! They read the `GameCtx` their page provides (`hooks::use_game_ctx`), whose
//! driver says who plays: `LocalGame` plays every seat on this device,
//! `OnlineGameView` mirrors a server-authoritative room.

mod game_hud;
mod game_over;
mod game_play;
mod player_hand;
#[cfg(test)]
mod rerender_tests;
mod shown_hand;

pub use game_over::GameOver;
pub use game_play::GamePlay;
