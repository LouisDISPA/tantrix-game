//! HTTP room allocation and permanent identity. IDs never contain session tokens.

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoom {
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomReference {
    pub id: String,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomApiError {
    InvalidRequest,
    NotFound,
    Expired,
    AllocationBusy,
    StorageUnavailable,
}

impl RoomApiError {
    pub fn status(self) -> u16 {
        match self {
            Self::InvalidRequest => 400,
            Self::NotFound => 404,
            Self::Expired => 410,
            Self::AllocationBusy | Self::StorageUnavailable => 503,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidRequest => "This room request is invalid.",
            Self::NotFound => "Room not found. Create a new room or check the code.",
            Self::Expired => {
                "This lobby expired because the game never started. Create a new room."
            }
            Self::AllocationBusy => "Couldn't find an available room code. Please try again.",
            Self::StorageUnavailable => "Room storage is unavailable. Please try again.",
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
