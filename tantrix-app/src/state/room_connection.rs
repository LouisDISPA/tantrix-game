//! One-shot room creation intent and the room page's captured server.

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
    #[cfg(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    ))]
    {
        *CREATE_ROOM
            .lock()
            .expect("room creation intent lock poisoned") = Some(room_code.to_owned());
    }
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
    #[cfg(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    ))]
    return CREATE_ROOM
        .lock()
        .expect("room creation intent lock poisoned")
        .take()
        .as_deref()
        == Some(room_code);
    #[cfg(not(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    )))]
    {
        let _ = room_code;
        false
    }
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|win| win.session_storage().ok().flatten())
}

#[cfg(any(
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android")
))]
// Android WebView events and room rendering can run on different threads.
// The creation intent must survive the handoff between them.
static CREATE_ROOM: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The server captured by a room page, shared with its invitation UI.
#[cfg(any(
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android")
))]
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

        // Model a native WebView click followed by rendering on another thread.
        std::thread::spawn(|| mark_create("BCDEFG")).join().unwrap();
        let create = take_create("BCDEFG");
        assert!(create);
        assert!(
            super::super::server_endpoints::room_url("https://example.com", "BCDEFG", create)
                .ends_with("/room/BCDEFG?create=1")
        );
        assert!(!take_create("BCDEFG"));
    }
}
