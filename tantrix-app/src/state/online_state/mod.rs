//! Reactive state for an online room.
//!
//! Stored as a `Store<OnlineState>` owned by `OnlineRoom`. The driver task
//! (spawned in [`crate::pages::online::driver`]) pumps
//! [`tantrix_net::SessionEvent`]s in by writing to individual Store field
//! signals — unrelated subscribers stay asleep.

mod phase;
pub(crate) mod queries;
pub(crate) mod recovery;
mod sender;
mod state;
#[cfg(test)]
pub(crate) mod test_rooms;

pub(crate) use phase::{reject_message, FailureReason, OnlinePhase};
pub(crate) use sender::CommandSender;
pub(crate) use state::{ConnectionStatus, OnlineState, OnlineStateStoreExt};
