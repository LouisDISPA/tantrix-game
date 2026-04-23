//! Per-room persistent session token.
//!
//! The token is a UUID v4 stored in `localStorage` under a room-scoped key.
//! Clients send it as part of `PeerMessage::Hello`, and the host uses it
//! to rebind a reconnecting player to their seat across a page reload —
//! something the ephemeral matchbox `PeerId` cannot do.

use tantrix_proto::SessionToken;

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const STORAGE_PREFIX: &str = "tantrix-token-v1:";

/// Return the stored token for `room_code`, or generate a fresh UUID v4 and
/// persist it. On non-web targets a new token is minted every call —
/// reconnection without storage isn't supported there, which is fine since
/// there's no page reload to survive.
pub fn load_or_create(room_code: &str) -> SessionToken {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = local_storage() {
            let key = storage_key(room_code);
            if let Ok(Some(existing)) = storage.get_item(&key) {
                let trimmed = existing.trim();
                if !trimmed.is_empty() {
                    return SessionToken::new(trimmed);
                }
            }
            let fresh = generate();
            let _ = storage.set_item(&key, fresh.as_str());
            return fresh;
        }
    }
    let _ = room_code;
    generate()
}

/// Delete the stored token for `room_code`. Called when the host kicks us
/// so a subsequent rejoin doesn't instantly re-trigger the kick-seen-before
/// path.
pub fn clear(room_code: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = local_storage() {
            let _ = storage.remove_item(&storage_key(room_code));
        }
    }
    let _ = room_code;
}

fn generate() -> SessionToken {
    SessionToken::new(uuid_v4_string())
}

/// Generate a UUID v4 string without pulling in the full `uuid` crate
/// transitively into the app. 16 random bytes with the standard variant
/// bits set, formatted canonically.
fn uuid_v4_string() -> String {
    let mut bytes = [0u8; 16];
    // Panic on RNG failure: the token is a bearer credential (the host
    // rebinds seats by it), so a predictable fallback would let two clients
    // collide onto the same participant record.
    getrandom::fill(&mut bytes).expect("OS RNG unavailable");
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn storage_key(room_code: &str) -> String {
    format!("{STORAGE_PREFIX}{room_code}")
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}
