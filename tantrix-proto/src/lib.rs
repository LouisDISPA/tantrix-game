//! Wire format for Tantrix online games.
//!
//! Two message enums flow over the WebRTC data channel between peers:
//!
//! * [`PeerMessage`] — what a non-host peer tells the host (intents).
//! * [`HostMessage`] — what the host broadcasts back to everyone (authoritative
//!   state updates, errors, lifecycle events).
//!
//! Only the host ever validates and mutates the [`tantrix_game::Game`]. Clients
//! render whatever snapshots arrive and send intents when the user interacts.

use enum_map::EnumMap;
use serde::{Deserialize, Serialize};
use tantrix_game::{Board, Color, Hand, Move};

/// Protocol version. Host and clients refuse to start a game if their values
/// differ; bump this whenever the wire format breaks compatibility.
pub const PROTO_VERSION: u32 = 1;

/// Opaque peer identifier. Mirrors the string form of `matchbox`'s `PeerId`
/// so the protocol stays transport-agnostic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub String);

impl PeerId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A seated player in the lobby or an active game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seat {
    pub peer_id: PeerId,
    pub nickname: String,
}

/// Lobby snapshot: who's in the room and which colors are claimed.
///
/// `seats[c]` is `Some(seat)` when a player has claimed color `c`. An empty
/// entry means the color is available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyState {
    pub host: PeerId,
    pub seats: EnumMap<Color, Option<Seat>>,
    /// Players connected but not yet seated.
    pub unseated: Vec<Seat>,
}

/// Authoritative game snapshot. Sent on every state transition; clients
/// simply replace their local copy.
///
/// The bag's contents are intentionally hidden — only `bag_count` ships —
/// so a malicious client can't peek at future draws.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSnapshot {
    pub board: Board,
    pub hands: EnumMap<Color, Hand>,
    pub bag_count: usize,
    pub current_player: Color,
    pub playing_colors: Vec<Color>,
    pub seats: EnumMap<Color, Option<Seat>>,
}

/// Why a game ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndReason {
    /// All tiles placed; `winners` holds the tied top-scoring colors.
    Completed { winners: Vec<Color> },
    /// A seated player disconnected past the reconnect window.
    PlayerDisconnected { color: Color },
    /// Host closed the room.
    HostLeft,
}

/// Client-side rejection reasons the host sends back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectCode {
    LobbyFull,
    ColorTaken,
    IncompatibleVersion,
    NotYourTurn,
    InvalidMove,
    GameInProgress,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// A non-host peer's message to the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerMessage {
    /// First message after the data channel opens.
    Hello {
        nickname: String,
        proto_version: u32,
    },
    /// Claim or release a color. `color: None` = leave the seat.
    PickColor {
        color: Option<Color>,
    },
    /// Host-only request to transition lobby → game.
    StartGame,
    /// Submit a move for the current turn.
    PlayMove {
        m: Move,
    },
    /// Current player has no legal moves and wants to pass.
    SkipTurn,
    /// Pull the latest snapshot (e.g. after reconnect).
    RequestSnapshot,
    /// Change my display name. Host updates `peers` and any seat binding
    /// and rebroadcasts the lobby so other clients see the new label.
    UpdateNickname {
        nickname: String,
    },
}

/// The host's broadcast to one or all peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostMessage {
    /// Host replies to a `Hello` with the peer's assigned nickname and the
    /// current lobby. If `rejected` is Some, the peer should close.
    Welcome {
        your_peer_id: PeerId,
        host_peer_id: PeerId,
        lobby: LobbyState,
        rejected: Option<RejectCode>,
    },
    /// Lobby changed (someone joined, picked a color, left, etc.).
    LobbyUpdate {
        lobby: LobbyState,
    },
    /// Lobby → playing transition. Clients should render the game view.
    GameStarted {
        snapshot: GameSnapshot,
    },
    /// New authoritative state after a move / skip.
    StateUpdate {
        snapshot: GameSnapshot,
        last_move: Option<Move>,
    },
    /// Terminal state. After this the host stops publishing updates.
    GameEnded {
        reason: EndReason,
        snapshot: GameSnapshot,
    },
    /// Per-peer rejection for the most recent intent.
    Error {
        code: RejectCode,
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

/// Encode any serializable message to a byte vector suitable for a matchbox
/// packet. We use JSON for readability during development; switching to a
/// binary codec later only requires changing these two functions.
pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    serde_json::to_vec(msg).expect("tantrix-proto messages are always serializable")
}

pub fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, serde_json::Error> {
    serde_json::from_slice(bytes)
}
