//! Browser and native WebSocket transports for server-authoritative Tantrix rooms.

mod client;
mod roomcode;
mod session;

pub use client::open_session;
pub use roomcode::generate_room_code;
pub use session::{SessionCommand, SessionEvent, SessionHandle};
pub use tantrix_proto::{normalize_room_code, ROOM_CODE_ALPHABET, ROOM_CODE_LEN};

pub use tantrix_proto;

#[cfg(all(not(target_arch = "wasm32"), feature = "native"))]
pub use native::open_native_session;
#[cfg(all(not(target_arch = "wasm32"), feature = "native"))]
mod native;
