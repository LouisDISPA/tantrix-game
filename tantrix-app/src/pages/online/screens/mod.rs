//! Leaf screens shown at various phases of the online session.

mod connecting;
mod failed;

pub(super) use connecting::ConnectingView;
pub(super) use failed::FailedView;
