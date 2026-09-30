//! Session lifecycle + reasons a session can end.

use tantrix_game::Move;
use tantrix_net::ConnectStage;
use tantrix_proto::{EndReason, GameSnapshot, LobbyState, RejectCode};

/// Top-level lifecycle of the online session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnlinePhase {
    /// Matchbox socket is being opened; no peer id yet.
    Connecting,
    /// Socket is live, we're in the lobby picking colors.
    Lobby { lobby: LobbyState },
    /// Game is running.
    Playing {
        snapshot: GameSnapshot,
        last_move: Option<Move>,
    },
    /// Terminal state. The snapshot carries the final board and seats.
    Ended {
        reason: EndReason,
        snapshot: GameSnapshot,
    },
    /// Fatal error; the session task has exited.
    Failed { reason: FailureReason },
}

/// Why the session ended. Kept structured rather than stringified so the UI
/// can pick the right heading (kicked / version mismatch / transport) without
/// sniffing substrings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureReason {
    /// Host sent a structured rejection.
    Rejected(RejectCode),
    /// Transport-level failure (socket closed, signaling error, etc.).
    Transport(String),
    /// Connecting stalled in the given stage; retrying usually works.
    Timeout(ConnectStage),
}

impl FailureReason {
    pub fn display(&self) -> String {
        match self {
            FailureReason::Rejected(code) => reject_message(code).to_string(),
            FailureReason::Transport(msg) => msg.clone(),
            FailureReason::Timeout(stage) => timeout_message(*stage).to_string(),
        }
    }

    /// Whether trying again could plausibly help. A host rejection (kicked,
    /// version mismatch, game already running) will just repeat itself.
    pub fn is_retryable(&self) -> bool {
        !matches!(self, FailureReason::Rejected(_))
    }
}

/// User-facing explanation for each stage a connection attempt can stall in.
pub fn timeout_message(stage: ConnectStage) -> &'static str {
    match stage {
        ConnectStage::Signaling => {
            "Couldn't reach the game server. Check your connection and try again."
        }
        ConnectStage::PeerLink => {
            "Couldn't open a direct connection to the host. This is usually a temporary \
             network hiccup, so trying again often works."
        }
        ConnectStage::Welcome => {
            "The host didn't respond. Make sure the room is still open, then try again."
        }
    }
}

/// Short user-facing sentence for each host-reject reason.
pub fn reject_message(code: &RejectCode) -> &'static str {
    match code {
        RejectCode::ColorTaken => "That color is taken.",
        RejectCode::IncompatibleVersion => "The host is running a different game version.",
        RejectCode::NotYourTurn => "It's not your turn.",
        RejectCode::InvalidMove => "The host rejected that move.",
        RejectCode::GameInProgress => "This room's game is already in progress.",
        RejectCode::Kicked => "You were removed from the room.",
    }
}
