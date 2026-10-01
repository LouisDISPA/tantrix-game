//! Session lifecycle + reasons a session can end.

use tantrix_game::Move;
use tantrix_proto::{EndReason, GameSnapshot, LobbyState, RejectCode};

/// Top-level lifecycle of the online session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnlinePhase {
    /// The room WebSocket is opening; no participant id yet.
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
/// can pick the right heading (room rejection / transport) without
/// sniffing substrings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureReason {
    /// Server sent a structured rejection.
    Rejected(RejectCode),
    /// Transport-level failure (socket closed, network error, etc.).
    Transport(String),
    /// Connecting stalled; retrying usually works.
    Timeout,
}

impl FailureReason {
    pub fn display(&self) -> String {
        match self {
            FailureReason::Rejected(code) => reject_message(code).to_string(),
            FailureReason::Transport(msg) => msg.clone(),
            FailureReason::Timeout => {
                "Couldn't reach the game server. Check your connection and try again.".into()
            }
        }
    }

    /// Whether trying again could plausibly help. A room rejection will repeat.
    pub fn is_retryable(&self) -> bool {
        !matches!(self, FailureReason::Rejected(_))
    }
}

/// Short user-facing sentence for each server rejection.
pub fn reject_message(code: &RejectCode) -> &'static str {
    match code {
        RejectCode::ColorTaken => "That color is taken.",
        RejectCode::NotYourTurn => "It's not your turn.",
        RejectCode::InvalidMove => "The game server rejected that move.",
        RejectCode::GameInProgress => "This game has already started.",
        RejectCode::RoomNotFound => "Room not found. Create a new room or check the code.",
        RejectCode::RoomFull => "This room is full.",
    }
}

#[cfg(test)]
mod tests {
    use tantrix_proto::RejectCode;

    use super::{reject_message, FailureReason};

    const CODES: [RejectCode; 6] = [
        RejectCode::ColorTaken,
        RejectCode::NotYourTurn,
        RejectCode::InvalidMove,
        RejectCode::GameInProgress,
        RejectCode::RoomNotFound,
        RejectCode::RoomFull,
    ];

    #[test]
    fn each_rejection_has_its_own_sentence() {
        let mut messages: Vec<_> = CODES.iter().map(reject_message).collect();
        assert!(messages.iter().all(|m| m.ends_with('.')));
        messages.sort_unstable();
        messages.dedup();
        assert_eq!(messages.len(), CODES.len());
    }

    #[test]
    fn a_failure_says_why_and_whether_to_retry() {
        let full = FailureReason::Rejected(RejectCode::RoomFull);
        assert_eq!(full.display(), "This room is full.");
        assert!(!full.is_retryable());

        let dropped = FailureReason::Transport("Connection closed.".to_string());
        assert_eq!(dropped.display(), "Connection closed.");
        assert!(dropped.is_retryable());

        assert!(FailureReason::Timeout
            .display()
            .starts_with("Couldn't reach the game server."));
        assert!(FailureReason::Timeout.is_retryable());
    }
}
