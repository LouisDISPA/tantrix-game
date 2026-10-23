//! The reactive `OnlineState` struct.
//!
//! Backed by a `Store` so each field has its own signal accessor — writing
//! `state.phase().set(..)` only wakes phase subscribers, not error or
//! participant-id subscribers. See [`super::queries`] for read-only derivations
//! that take narrow slices of this state and can be memoed cheaply.

use dioxus::prelude::*;
use tantrix_proto::{PeerId, RejectCode};

use super::phase::OnlinePhase;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Reconnecting { attempt: usize },
    Stopped,
    OpenedElsewhere,
}

impl ConnectionStatus {
    pub fn is_connected(self) -> bool {
        self == Self::Connected
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Store)]
pub struct OnlineState {
    pub room_code: String,
    pub my_peer_id: Option<PeerId>,
    pub phase: OnlinePhase,
    pub connection: ConnectionStatus,
    pub seat_changed: bool,
    /// Last error shown to the user, cleared on next successful update.
    pub last_error: Option<RejectCode>,
}

impl OnlineState {
    pub fn new(room_code: String) -> Self {
        Self {
            room_code,
            my_peer_id: None,
            phase: OnlinePhase::Connecting,
            connection: ConnectionStatus::Connecting,
            seat_changed: false,
            last_error: None,
        }
    }
}
