//! Room rules, orchestration and browser identity shared by the Worker and
//! the native server: [`room_logic`] applies intents to a room,
//! [`engine`] turns socket events into effects without I/O, and
//! [`handshake`] and [`session_cookie`] check the socket upgrade.

pub mod engine;
pub mod handshake;
pub mod room_logic;
pub mod session_cookie;
