//! Same-origin room WebSocket URL and one-shot room creation intent.

#[cfg(target_arch = "wasm32")]
const CREATE_PREFIX: &str = "tantrix-create-room:";

/// Mark the next visit to this room as its creation attempt. Keeping the flag
/// in sessionStorage lets navigation finish before the socket opens, while a
/// page reload reconnects to the existing room rather than recreating it.
pub fn mark_create(room_code: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(storage) = storage() {
        let _ = storage.set_item(&format!("{CREATE_PREFIX}{room_code}"), "1");
    }
    #[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
    CREATE_ROOM.with(|pending| *pending.borrow_mut() = Some(room_code.to_owned()));
    let _ = room_code;
}

pub fn take_create(room_code: &str) -> bool {
    #[cfg(target_arch = "wasm32")]
    if let Some(storage) = storage() {
        let key = format!("{CREATE_PREFIX}{room_code}");
        let create = storage.get_item(&key).ok().flatten().is_some();
        let _ = storage.remove_item(&key);
        return create;
    }
    #[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
    return CREATE_ROOM.with(|pending| pending.borrow_mut().take().as_deref() == Some(room_code));
    #[cfg(not(all(feature = "desktop", not(target_arch = "wasm32"))))]
    {
        let _ = room_code;
        false
    }
}

pub fn room_url(room_code: &str, create: bool) -> String {
    #[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
    let base = default_base();
    #[cfg(not(all(feature = "desktop", not(target_arch = "wasm32"))))]
    let base = option_env!("TANTRIX_ROOM_URL")
        .map(str::to_string)
        .unwrap_or_else(default_base);
    let query = if create { "?create=1" } else { "" };
    format!("{}/{room_code}{query}", base.trim_end_matches('/'))
}

#[cfg(target_arch = "wasm32")]
fn default_base() -> String {
    let Some(win) = web_sys::window() else {
        return "ws://localhost:8787/room".into();
    };
    let loc = win.location();
    let scheme = if loc.protocol().as_deref() == Ok("https:") {
        "wss"
    } else {
        "ws"
    };
    let host = loc.host().unwrap_or_else(|_| "localhost:8787".into());
    format!("{scheme}://{host}/room")
}

#[cfg(not(target_arch = "wasm32"))]
fn default_base() -> String {
    #[cfg(feature = "desktop")]
    {
        format!(
            "{}/room",
            super::desktop_settings::server_address().replacen("http", "ws", 1)
        )
    }
    #[cfg(not(feature = "desktop"))]
    {
        "ws://localhost:8787/room".into()
    }
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|win| win.session_storage().ok().flatten())
}

#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
thread_local! {
    static CREATE_ROOM: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// The server captured by a room page, shared with its invitation UI.
#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
#[derive(Clone)]
pub struct RoomServer(pub String);

#[cfg(all(test, feature = "desktop", not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn room_creation_is_consumed_once() {
        mark_create("ABCDEF");
        assert!(take_create("ABCDEF"));
        assert!(!take_create("ABCDEF"));
        mark_create("ABCDEF");
        assert!(!take_create("BCDEFG"));
        assert!(!take_create("ABCDEF"));
    }
}
