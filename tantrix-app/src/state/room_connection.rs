//! The server captured by a room page, shared with its invitation UI.
#[cfg(any(
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android")
))]
#[derive(Clone)]
pub struct RoomServer(pub String);
