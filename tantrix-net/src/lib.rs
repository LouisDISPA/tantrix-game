//! Browser and native WebSocket transports for server-authoritative Tantrix rooms.

mod client;
#[cfg(any(target_arch = "wasm32", feature = "native", test))]
mod heartbeat;
mod session;

pub use client::open_session;
pub use session::{SessionCommand, SessionEvent, SessionHandle};
pub use tantrix_proto::{normalize_room_code, ROOM_CODE_ALPHABET, ROOM_CODE_LEN};

pub use tantrix_proto;

#[cfg(all(not(target_arch = "wasm32"), feature = "native"))]
pub use native::open_native_session;
#[cfg(all(not(target_arch = "wasm32"), feature = "native"))]
mod native;
