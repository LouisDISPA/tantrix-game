//! Reactive state for an online room.
//!
//! Stored as a `Store<OnlineState>` owned by `OnlineRoom`. The driver task
//! (spawned in [`crate::pages::online::driver`]) pumps
//! [`tantrix_net::SessionEvent`]s in by writing to individual Store field
//! signals — unrelated subscribers stay asleep.

mod phase;
pub mod queries;
mod sender;
mod state;

pub use phase::{reject_message, FailureReason, OnlinePhase};
pub use sender::CommandSender;
pub use state::{OnlineState, OnlineStateStoreExt};
