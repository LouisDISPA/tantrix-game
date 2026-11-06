//! The server captured by a room page, shared with its invitation UI.
#[cfg(native_io)]
#[derive(Clone)]
pub struct RoomServer(pub String);
