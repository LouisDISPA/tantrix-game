//! Networking layer for online Tantrix.
//!
//! Wraps [`matchbox_socket`] with a small state machine on top:
//!
//! * The **host** owns a [`tantrix_game::Game`] and a lobby, validates every
//!   intent, and broadcasts [`tantrix_proto::HostMessage`]s.
//! * The **client** is a thin mirror — it sends [`tantrix_proto::PeerMessage`]
//!   intents to the host and applies whatever snapshots come back.
//!
//! Both roles expose the same [`SessionHandle`] API: send
//! [`SessionCommand`]s, receive [`SessionEvent`]s. The event stream is the
//! single source of truth for the UI layer.

mod client;
mod host;
mod roomcode;
mod session;
mod socket;

pub use client::open_client_session;
pub use host::open_host_session;
pub use roomcode::{generate_room_code, normalize_room_code, ROOM_CODE_ALPHABET, ROOM_CODE_LEN};
pub use session::{ConnectStage, SessionCommand, SessionEvent, SessionHandle, SessionRole};

pub use matchbox_socket::PeerId as MatchboxPeerId;
pub use tantrix_proto;
pub use tantrix_proto::SessionToken;
