//! Browser WebSocket transport for server-authoritative Tantrix rooms.

mod client;
mod roomcode;
mod session;

pub use client::open_session;
pub use roomcode::{generate_room_code, normalize_room_code, ROOM_CODE_ALPHABET, ROOM_CODE_LEN};
pub use session::{SessionCommand, SessionEvent, SessionHandle};

pub use tantrix_proto;
