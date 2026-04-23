//! How the client chooses which signaling server to talk to.
//!
//! Default (fullstack): derive from `window.location` so the wasm bundle
//! connects back to the same origin that served it at `/signaling`. This
//! means `https://example.com` → `wss://example.com/signaling`, and local
//! dev on `http://localhost:8080` → `ws://localhost:8080/signaling`.
//!
//! Override: set `TANTRIX_SIGNALING_URL` at compile time to point the wasm
//! at a different signaling server (useful when running the frontend dev
//! server against a remote backend).

/// Compile-time override. When set, skips the runtime `window.location`
/// derivation.
const OVERRIDE_URL: Option<&str> = option_env!("TANTRIX_SIGNALING_URL");

/// Signaling URL for a given room code: `{base}/{ROOMCODE}`.
pub fn signaling_url(room_code: &str) -> String {
    let base = OVERRIDE_URL.map(str::to_string).unwrap_or_else(default_base);
    format!("{}/{}", base.trim_end_matches('/'), room_code)
}

#[cfg(target_arch = "wasm32")]
fn default_base() -> String {
    let Some(win) = web_sys::window() else {
        return "ws://localhost:8080/signaling".to_string();
    };
    let loc = win.location();
    let scheme = match loc.protocol().as_deref() {
        Ok("https:") => "wss",
        _ => "ws",
    };
    let host = loc.host().unwrap_or_else(|_| "localhost:8080".to_string());
    format!("{scheme}://{host}/signaling")
}

#[cfg(not(target_arch = "wasm32"))]
fn default_base() -> String {
    "ws://localhost:8080/signaling".to_string()
}
