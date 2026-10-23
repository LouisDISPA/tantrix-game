//! Shared validation for permanent-ID room connections.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    InvalidCode,
    StorageUnavailable,
}
impl Refusal {
    pub fn status(self) -> u16 {
        match self {
            Self::InvalidCode => 400,
            Self::StorageUnavailable => 503,
        }
    }
    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidCode => "invalid game ID",
            Self::StorageUnavailable => "room storage is unavailable",
        }
    }
}
pub fn check(id: &str) -> Result<(), Refusal> {
    if !tantrix_proto::is_game_id(id) {
        return Err(Refusal::InvalidCode);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permanent_ids_are_validated_before_opening_a_room() {
        assert_eq!(check("01234567-89ab-4def-8123-456789abcdef"), Ok(()));
        assert_eq!(check("ABCDEF"), Err(Refusal::InvalidCode));
    }
}
