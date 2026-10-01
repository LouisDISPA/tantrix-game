//! 6-character uppercase room codes, as they appear in `/room/{CODE}` and
//! `/join/{CODE}`. The client normalizes what a player types with
//! [`normalize_room_code`]; both room servers accept only
//! [`is_room_code`] codes. (`cloudflare/src/index.js` and the e2e specs
//! repeat the alphabet in JavaScript.)
//!
//! The alphabet excludes visually ambiguous glyphs (`I`, `O`, `Q`) so codes
//! stay legible when shared verbally. 23⁶ ≈ 148 M values — plenty for the
//! concurrent-room scales we care about.

pub const ROOM_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPRSTUVWXYZ";
pub const ROOM_CODE_LEN: usize = 6;

/// Validate a code the user typed. Returns the normalized (uppercased) form
/// if every character is in the alphabet, else `None`.
pub fn normalize_room_code(input: &str) -> Option<String> {
    let upper = input.trim().to_ascii_uppercase();
    is_room_code(&upper).then_some(upper)
}

/// Whether `code` is a room code exactly as the client sends it: already
/// normalized, so one room never answers to two spellings.
pub fn is_room_code(code: &str) -> bool {
    code.len() == ROOM_CODE_LEN && code.bytes().all(|b| ROOM_CODE_ALPHABET.contains(&b))
}

#[cfg(test)]
mod tests {
    use super::{is_room_code, normalize_room_code};

    #[test]
    fn normalizes_valid_room_codes() {
        assert_eq!(normalize_room_code(" abchjm "), Some("ABCHJM".into()));
    }

    #[test]
    fn rejects_ambiguous_or_wrong_length_room_codes() {
        assert_eq!(normalize_room_code("ABCIOQ"), None);
        assert_eq!(normalize_room_code("ABCDE"), None);
    }

    #[test]
    fn servers_accept_only_the_normalized_spelling() {
        assert!(is_room_code("ABCHJM"));
        assert!(!is_room_code("abchjm"));
        assert!(!is_room_code(" ABCHJM"));
        assert!(!is_room_code("ABCHJMK"));
    }
}
