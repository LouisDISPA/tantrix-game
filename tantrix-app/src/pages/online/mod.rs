//! Online multiplayer pages: entry, lobby, and in-game view.

mod driver;
mod entry;
mod game;
mod hud_badge;
mod lobby;
mod room;
mod seat_takeover;
mod screens;

pub use entry::{JoinRoom, OnlineEntry};
pub use room::OnlineRoom;
