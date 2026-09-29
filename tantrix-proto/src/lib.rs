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
pub const PROTO_VERSION: u32 = 2;

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

/// Client-generated, per-room persistent identity. Survives reload and
/// reconnect so the host can rebind a returning peer to its seat without
/// relying on the ephemeral matchbox `PeerId`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionToken(pub String);

impl SessionToken {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A seated player in the lobby or an active game.
///
/// Kept on the wire for backward-friendly UI rendering. The authoritative
/// roster is [`Participant`]; `Seat` is the subset visible in `LobbyState`
/// / `GameSnapshot` for code paths that only care about colored seats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seat {
    pub peer_id: PeerId,
    pub nickname: String,
}

/// Role of a participant in the room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParticipantRole {
    /// A colored seat in the current / upcoming game.
    Seated(Color),
    /// Watching only; can never submit moves.
    Spectator,
    /// The host. Implicitly the authoritative peer.
    Host,
    /// Joined but not yet seated (lobby only, before the game starts).
    Unseated,
}

/// A single member of the room — seated, spectator, or host — with the
/// information needed to render the participants panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub peer_id: PeerId,
    pub token: SessionToken,
    pub nickname: String,
    pub role: ParticipantRole,
    /// Which color (if any) this participant is currently seated in. Mirrors
    /// `seats[c] == this.peer_id` so the UI can render a consistent color
    /// dot regardless of the `role` enum (e.g. a host who picked a color
    /// still has `role = Host` but `seated_color = Some(..)`).
    pub seated_color: Option<Color>,
    /// `true` if the host currently holds a live transport channel to this
    /// peer. Disconnected-but-remembered participants have `false` and
    /// keep their seat reserved.
    pub connected: bool,
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
    /// Authoritative roster (seated + unseated + spectators). Superset of
    /// `seats` + `unseated`; drives the participants panel.
    pub participants: Vec<Participant>,
}

/// Authoritative game snapshot. Sent on every state transition; clients
/// simply replace their local copy.
///
/// The bag's contents are intentionally hidden — only `bag_count` ships —
/// so a malicious client can't peek at future draws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSnapshot {
    pub board: Board,
    pub hands: EnumMap<Color, Hand>,
    pub bag_count: usize,
    pub current_player: Color,
    pub playing_colors: Vec<Color>,
    pub seats: EnumMap<Color, Option<Seat>>,
    /// Full roster including spectators and disconnected-but-remembered
    /// seated players. Drives the in-game participants panel.
    pub participants: Vec<Participant>,
}

/// Why a game ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndReason {
    /// All tiles placed; `winners` holds the tied top-scoring colors.
    Completed { winners: Vec<Color> },
    /// Host closed the room.
    HostLeft,
}

/// Client-side rejection reasons the host sends back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectCode {
    ColorTaken,
    IncompatibleVersion,
    NotYourTurn,
    InvalidMove,
    GameInProgress,
    Kicked,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// A non-host peer's message to the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerMessage {
    /// First message after the data channel opens. The `session_token` lets
    /// the host rebind a returning peer to an existing seat across reload.
    Hello {
        nickname: String,
        proto_version: u32,
        session_token: SessionToken,
    },
    /// Claim or release a color. `color: None` = leave the seat.
    PickColor { color: Option<Color> },
    /// Host-only request to transition lobby → game.
    StartGame,
    /// Submit a move for the current turn.
    PlayMove { m: Move },
    /// Current player has no legal moves and wants to pass.
    SkipTurn,
    /// Pull the latest snapshot (e.g. after reconnect).
    RequestSnapshot,
    /// Change my display name. Host updates the roster and any seat binding
    /// and rebroadcasts the lobby so other clients see the new label.
    UpdateNickname { nickname: String },
    /// Host-only: remove a participant from the room.
    Kick { target: PeerId },
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
    LobbyUpdate { lobby: LobbyState },
    /// Lobby → playing transition. Clients should render the game view.
    GameStarted { snapshot: GameSnapshot },
    /// New authoritative state after a move / skip / roster change.
    StateUpdate {
        snapshot: GameSnapshot,
        last_move: Option<Move>,
    },
    /// Terminal state. After this the host stops publishing updates.
    GameEnded {
        reason: EndReason,
        snapshot: GameSnapshot,
    },
    /// Host removed someone. Every peer receives this; the targeted peer
    /// should close its session and clear its stored token for the room.
    Kicked { target: PeerId, reason: String },
    /// Per-peer rejection for the most recent intent.
    Error { code: RejectCode, message: String },
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
