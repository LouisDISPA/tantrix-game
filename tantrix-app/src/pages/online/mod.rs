//! Online multiplayer pages: entry, lobby, and in-game view.

mod driver;
mod entry;
mod game;
mod hud_badge;
mod lobby;
mod room;
mod screens;
mod seat_takeover;

pub use entry::{JoinRoom, OnlineEntry};
pub use room::OnlineRoom;
