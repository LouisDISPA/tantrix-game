//! Wire format for Tantrix online games.
//!
//! Two message enums flow over a WebSocket between clients and a room server:
//!
//! * [`PeerMessage`] — client intents.
//! * [`HostMessage`] — server responses and broadcasts (authoritative
//!   state updates, errors, lifecycle events).
//!
//! Only the server validates and mutates the [`tantrix_game::Game`]. Clients
//! render whatever snapshots arrive and send intents when the user interacts.

use enum_map::EnumMap;
use serde::{Deserialize, Serialize};
use tantrix_game::{Board, Color, Hand, Move, Placement, TurnStep};

pub mod room_code;

pub use room_code::{is_room_code, normalize_room_code, ROOM_CODE_ALPHABET, ROOM_CODE_LEN};

/// Raw keepalive frames. The room answers the ping with the pong through the
/// runtime's auto-response, without waking the Durable Object.
pub const KEEPALIVE_PING: &str = "ping";
pub const KEEPALIVE_PONG: &str = "pong";

/// Public, server-generated participant identifier.
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
    /// In the room without a color: waiting for a seat, or watching a game.
    Unseated,
}

/// A single member of the room — seated, spectator, or host — with the
/// information needed to render the participants panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub peer_id: PeerId,
    pub nickname: String,
    pub role: ParticipantRole,
    /// Mirrors `seats[c] == this.peer_id` for the UI color dot.
    pub seated_color: Option<Color>,
    /// Disconnected players retain their seat after a game starts.
    pub connected: bool,
}

/// Lobby snapshot: who's in the room and which colors are claimed.
///
/// `seats[c]` is `Some(seat)` when a player has claimed color `c`. An empty
/// entry means the color is available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyState {
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
    /// After the free move the current player may only fill forced spaces.
    #[serde(default)]
    pub turn_step: TurnStep,
    pub playing_colors: Vec<Color>,
    pub seats: EnumMap<Color, Option<Seat>>,
    /// Full roster including disconnected players. Drives the in-game panel.
    pub participants: Vec<Participant>,
}

/// Why a game ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndReason {
    /// All tiles placed; `winners` holds the tied top-scoring colors.
    Completed { winners: Vec<Color> },
    /// A seated player ended the game before all tiles were placed.
    EndedEarly { by: Color, winners: Vec<Color> },
}

/// Client-side rejection reasons the server sends back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectCode {
    ColorTaken,
    NotYourTurn,
    InvalidMove,
    GameInProgress,
    RoomNotFound,
    /// The room already holds its maximum number of members.
    RoomFull,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// A client's message to the room server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerMessage {
    /// First message after the WebSocket opens. The player's identity is
    /// not in the message: the room reads it from the HttpOnly session
    /// cookie on the WebSocket handshake, which is how a reload or
    /// reconnect rebinds a returning player to their seat.
    Hello { nickname: String },
    /// Claim a free color or release mine (`color: None`). A color someone
    /// else holds is rejected with `ColorTaken`, so two players racing for
    /// the same free seat can't silently replace each other.
    PickColor { color: Option<Color> },
    /// Deliberately take `color` from whoever holds it (the UI asks for
    /// confirmation first). In the lobby anyone may do this; during a game
    /// only someone without a seat can, and inherits that seat's hand and
    /// turn. The replaced player stays in the room without a seat.
    ReplaceSeat { color: Color },
    /// A seated player's request to transition lobby → game.
    StartGame,
    /// Place a tile for the current turn. The server checks it against the
    /// legal moves; whether it fills a forced space is the board's call.
    PlayMove { placement: Placement },
    /// Current player has no legal moves and wants to pass.
    SkipTurn,
    /// Finish the running game and score the board as it stands.
    EndGame,
    /// Pull the latest snapshot (e.g. after reconnect).
    RequestSnapshot,
    /// Change my display name and publish the updated roster.
    UpdateNickname { nickname: String },
}

/// The server's response or broadcast to one or all peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostMessage {
    /// Server replies to `Hello`. If `rejected` is Some, the client should close.
    Welcome {
        your_peer_id: PeerId,
        lobby: LobbyState,
        rejected: Option<RejectCode>,
    },
    /// Lobby changed (someone joined, picked a color, left, etc.).
    LobbyUpdate { lobby: LobbyState },
    /// Lobby → playing transition. Clients should render the game view.
    GameStarted { snapshot: GameSnapshot },
    /// New authoritative state after a move / skip / roster change.
    /// `last_move` is the move as the server placed it.
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
    Error { code: RejectCode, message: String },
}

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

/// Encode any serializable message to a byte vector suitable for a WebSocket
/// frame. We use JSON for readability during development; switching to a
/// binary codec later only requires changing these two functions.
pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    serde_json::to_vec(msg).expect("tantrix-proto messages are always serializable")
}

pub fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, serde_json::Error> {
    serde_json::from_slice(bytes)
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Direction, Placement, Position, Rotation, Tile};

    use super::{decode, encode, PeerMessage};

    /// Clients send a placement with no `forced` flag. The e2e suite's raw
    /// room sockets (`e2e/tests/online-authority.spec.ts`) write this JSON
    /// by hand, so the shape is part of the protocol.
    #[test]
    fn play_move_sends_a_bare_placement() {
        let placement = Placement {
            tile: Tile::RYYBRB,
            position: Position::CENTER.neighbor(Direction::Left),
            rotation: Rotation::R60,
        };
        let json = r#"{"PlayMove":{"placement":{"tile":"RYYBRB","position":{"x":-1,"y":0},"rotation":"R60"}}}"#;

        let encoded = encode(&PeerMessage::PlayMove { placement });
        assert_eq!(String::from_utf8(encoded).unwrap(), json);
        let decoded: PeerMessage = decode(json.as_bytes()).unwrap();
        assert!(matches!(decoded, PeerMessage::PlayMove { placement: p } if p == placement));
    }
}
