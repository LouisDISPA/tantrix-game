//! Leaf screens shown at various phases of the online session.

mod connecting;
mod failed;
mod waiting_for_game;

pub use connecting::ConnectingView;
pub use failed::FailedView;
pub use waiting_for_game::WaitingForGame;
