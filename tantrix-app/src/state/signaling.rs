//! Where the matchbox_server lives.
//!
//! Overridable at compile time via `TANTRIX_SIGNALING_URL`. The default
//! points at a locally running `matchbox_server` bound to its standard
//! port. Production deployments should rebuild with the env var set to a
//! `wss://` URL.

const DEFAULT_SIGNALING_URL: &str = match option_env!("TANTRIX_SIGNALING_URL") {
    Some(v) => v,
    None => "ws://localhost:3536",
};

/// Signaling URL for a given room code: `{base}/{ROOMCODE}`.
pub fn signaling_url(room_code: &str) -> String {
    format!("{}/{}", DEFAULT_SIGNALING_URL.trim_end_matches('/'), room_code)
}
