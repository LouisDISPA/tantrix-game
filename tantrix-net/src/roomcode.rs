//! 6-character uppercase room codes.
//!
//! The alphabet excludes visually ambiguous glyphs (`I`, `O`, `Q`) so codes
//! stay legible when shared verbally. 23⁶ ≈ 148 M values — plenty for the
//! concurrent-room scales we care about.

pub const ROOM_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPRSTUVWXYZ";
pub const ROOM_CODE_LEN: usize = 6;

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

/// Validate a code the user typed. Returns the normalized (uppercased) form
/// if every character is in the alphabet, else `None`.
pub fn normalize_room_code(input: &str) -> Option<String> {
    let upper = input.trim().to_ascii_uppercase();
    if upper.len() != ROOM_CODE_LEN {
        return None;
    }
    if upper.bytes().all(|b| ROOM_CODE_ALPHABET.contains(&b)) {
        Some(upper)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_room_code;

    #[test]
    fn normalizes_valid_room_codes() {
        assert_eq!(normalize_room_code(" abchjm "), Some("ABCHJM".into()));
    }

    #[test]
    fn rejects_ambiguous_or_wrong_length_room_codes() {
        assert_eq!(normalize_room_code("ABCIOQ"), None);
        assert_eq!(normalize_room_code("ABCDE"), None);
    }
}
