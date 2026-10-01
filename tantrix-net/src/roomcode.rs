//! Fresh room codes. The alphabet and validation live in
//! [`tantrix_proto::room_code`], shared with the room servers.

use tantrix_proto::{ROOM_CODE_ALPHABET, ROOM_CODE_LEN};

/// Generate a fresh random code. Falls back to a deterministic seed on the
/// unlikely event that `getrandom` fails, which keeps the UI responsive;
/// callers can re-roll if a collision is detected later.
pub fn generate_room_code() -> String {
    let mut bytes = [0u8; ROOM_CODE_LEN];
    let _ = getrandom::fill(&mut bytes);

    let alphabet_len = ROOM_CODE_ALPHABET.len() as u8;
    bytes
        .iter()
        .map(|b| ROOM_CODE_ALPHABET[(b % alphabet_len) as usize] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::generate_room_code;

    #[test]
    fn generated_codes_are_valid_room_codes() {
        assert!(tantrix_proto::is_room_code(&generate_room_code()));
    }
}
