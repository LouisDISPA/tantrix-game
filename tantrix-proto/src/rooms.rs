//! HTTP room allocation and permanent identity. IDs never contain session tokens.

use serde::{Deserialize, Serialize};

/// Whether `id` has the shape of a lowercase hyphenated UUID (8-4-4-4-12 hex
/// digits), the one spelling accepted for room IDs and creation request IDs.
pub fn is_game_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}

/// Clients retry an unanswered `CreateRoom` with the same `request_id` for
/// at most this long after making it; servers keep tombstones longer.
pub const CREATE_RETRY_WINDOW_MS: u64 = 60 * 60 * 1000;

/// JSON body of `POST /api/rooms`, which creates a room.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoom {
    /// A random UUID the client picks per new room (see [`is_game_id`]).
    /// The server derives the room ID from it and the session, so a retry
    /// gets the same room.
    pub request_id: String,
}

/// A room's permanent ID and current invitation code, as the room API
/// answers and [`HostMessage::RoomInfo`](crate::HostMessage::RoomInfo)
/// carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomReference {
    /// The permanent room ID (see [`is_game_id`]).
    pub id: String,
    /// The invitation code players can join with; `None` while the room has
    /// none, e.g. once nobody has been connected for a while.
    pub code: Option<String>,
}

/// Why the HTTP room API refused a request; the JSON body of its error
/// responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomApiError {
    /// A malformed body, request ID, room ID or code.
    InvalidRequest,
    /// No room has that ID or code, or it has been deleted.
    NotFound,
    /// The lobby expired because its game never started.
    Expired,
    /// No free invitation code was found after several attempts.
    AllocationBusy,
    /// Room storage failed; the request may succeed if retried.
    StorageUnavailable,
    /// This client created or looked up too many rooms lately; retry later.
    RateLimited,
}

impl RoomApiError {
    /// HTTP status code of the response carrying this error.
    pub fn status(self) -> u16 {
        match self {
            Self::InvalidRequest => 400,
            Self::NotFound => 404,
            Self::Expired => 410,
            Self::RateLimited => 429,
            Self::AllocationBusy | Self::StorageUnavailable => 503,
        }
    }

    /// A player-facing sentence explaining the error; also its `Display` text.
    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidRequest => "This room request is invalid.",
            Self::NotFound => "Room not found. Create a new room or check the code.",
            Self::Expired => {
                "This lobby expired because the game never started. Create a new room."
            }
            Self::AllocationBusy => "Couldn't find an available room code. Please try again.",
            Self::StorageUnavailable => "Room storage is unavailable. Please try again.",
            Self::RateLimited => "Too many room requests. Wait a minute and try again.",
        }
    }
}

impl std::fmt::Display for RoomApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}
impl std::error::Error for RoomApiError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_have_one_canonical_spelling() {
        assert!(is_game_id("01234567-89ab-4def-8123-456789abcdef"));
        for id in [
            "ABCDEF",
            "01234567-89AB-4def-8123-456789abcdef",
            "0123456789ab-4def-8123-456789abcdef",
        ] {
            assert!(!is_game_id(id));
        }
    }
}
