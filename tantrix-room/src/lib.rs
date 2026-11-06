//! Room rules, orchestration and browser identity shared by the Worker and
//! the native server: [`room_logic`] applies intents to a room,
//! [`engine`] turns socket events into effects without I/O,
//! [`handshake`] and [`session_cookie`] check the socket upgrade, and
//! [`rate_limit`] throttles the room HTTP endpoints per client.

#![warn(missing_docs)]

pub mod engine;
pub mod handshake;
pub mod lifecycle;
pub mod rate_limit;
pub mod room_logic;
pub mod session_cookie;
