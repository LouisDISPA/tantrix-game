//! Online multiplayer pages: entry, lobby, and in-game view.

mod driver;
mod entry;
mod game;
mod lobby;
mod root;

pub use entry::{JoinRoom, OnlineEntry};
pub use root::OnlineRoom;
