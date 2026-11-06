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

#![warn(missing_docs)]

use enum_map::EnumMap;
use serde::{Deserialize, Serialize};
use tantrix_game::{Board, Color, Hand, Move, Placement, TurnStep};

pub mod room_code;
pub mod rooms;
pub use rooms::{is_game_id, CreateRoom, RoomApiError, RoomReference, CREATE_RETRY_WINDOW_MS};

pub use room_code::{is_room_code, normalize_room_code, ROOM_CODE_ALPHABET, ROOM_CODE_LEN};

/// Online compatibility, independent of app releases. Bump this for breaking
/// wire-format changes or changes to shared game rules (legal moves/scoring).
pub const ONLINE_VERSION: u32 = 3;
/// HTTP `GET` path where a server answers with its [`OnlineVersion`] as JSON,
/// so clients can check compatibility before joining a room.
pub const ONLINE_VERSION_PATH: &str = "/api/online-version";
/// Query parameter of the room socket URL carrying the client's
/// [`ONLINE_VERSION`]; servers refuse other versions.
pub const ONLINE_VERSION_PARAM: &str = "v";

/// Keep this endpoint's shape stable so incompatible clients can read it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OnlineVersion {
    /// The server's [`ONLINE_VERSION`]; clients compare it with their own.
    pub online_version: u32,
}

impl OnlineVersion {
    /// This build's version.
    pub const CURRENT: Self = Self {
        online_version: ONLINE_VERSION,
    };
}

/// Raw keepalive frames. The room answers the ping with the pong through the
/// runtime's auto-response, without waking the Durable Object.
pub const KEEPALIVE_PING: &str = "ping";
/// The room's reply to each [`KEEPALIVE_PING`].
pub const KEEPALIVE_PONG: &str = "pong";
/// The older socket is deliberately displaced by another tab of this session.
pub const OPENED_ELSEWHERE: &str = "This room was opened in another tab.";

/// Public, server-generated participant identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub String);

impl PeerId {
    /// Wraps `s` as a participant ID.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrows the ID as text.
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
    /// The seat holder's participant ID.
    pub peer_id: PeerId,
    /// The seat holder's display name.
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

/// A single member of the room, seated or not, and whether they are
/// connected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    /// The member's participant ID.
    pub peer_id: PeerId,
    /// Display name, as the room cleaned it.
    pub nickname: String,
    /// Whether the member holds a color, and which.
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
    /// Number of the room's current match, counting from 0; each rematch
    /// adds one.
    pub match_id: u32,
    /// The member holding each color.
    pub seats: EnumMap<Color, Option<Seat>>,
    /// Players connected but not yet seated.
    pub unseated: Vec<Seat>,
    /// Authoritative roster (seated + unseated). Superset of `seats` +
    /// `unseated`; says who is connected.
    pub participants: Vec<Participant>,
}

/// Authoritative game snapshot. Sent on every state transition; clients
/// simply replace their local copy.
///
/// The bag's contents are intentionally hidden — only `bag_count` ships —
/// so a malicious client can't peek at future draws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSnapshot {
    /// The match this game belongs to; see [`LobbyState::match_id`].
    pub match_id: u32,
    /// The tiles placed so far.
    pub board: Board,
    /// Each playing color's hand; empty for colors not in the game.
    pub hands: EnumMap<Color, Hand>,
    /// Number of tiles still in the bag.
    pub bag_count: usize,
    /// The color whose turn it is.
    pub current_player: Color,
    /// After the free move the current player may only fill forced spaces.
    #[serde(default)]
    pub turn_step: TurnStep,
    /// The colors in the game, in turn order.
    pub playing_colors: Vec<Color>,
    /// The member holding each color; `None` for colors not in the game.
    pub seats: EnumMap<Color, Option<Seat>>,
    /// Full roster including disconnected players. Drives the in-game panel.
    pub participants: Vec<Participant>,
}

/// Why a game ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndReason {
    /// All tiles placed; `winners` holds the tied top-scoring colors.
    Completed {
        /// The colors tied for the top score.
        winners: Vec<Color>,
    },
    /// A seated player ended the game before all tiles were placed.
    EndedEarly {
        /// The color of the player who ended it.
        by: Color,
        /// The colors tied for the top score on the board as it stood.
        winners: Vec<Color>,
    },
}

/// Client-side rejection reasons the server sends back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectCode {
    /// Another member holds that color (during a game: one still connected).
    ColorTaken,
    /// A move from someone other than the current player, or after the game
    /// ended.
    NotYourTurn,
    /// Any other refused intent, such as an illegal move, a stale
    /// `match_id` or an invalid nickname.
    InvalidMove,
    /// A lobby intent (picking a color, starting) after the game started.
    GameInProgress,
    /// No room has that ID: it never existed or has been deleted.
    RoomNotFound,
    /// The lobby expired because its game never started.
    RoomExpired,
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
    Hello {
        /// Display name for a newcomer; a returning player keeps their own.
        nickname: String,
    },
    /// Claim a free color or release mine (`color: None`). A color someone
    /// else holds is rejected with `ColorTaken`, so two players racing for
    /// the same free seat can't silently replace each other.
    PickColor {
        /// The color to claim, or `None` to give up the current one.
        color: Option<Color>,
    },
    /// Deliberately take `color` from whoever holds it (the UI asks for
    /// confirmation first). In the lobby anyone may do this; during a game
    /// only someone without a seat can, from a player who is not connected
    /// (`ColorTaken` otherwise), and inherits that seat's hand and turn. The
    /// replaced player stays in the room without a seat if connected.
    ReplaceSeat {
        /// The color to take over.
        color: Color,
    },
    /// A seated player's request to transition lobby → game.
    StartGame {
        /// The match to start; the room refuses any other.
        match_id: u32,
    },
    /// Place a tile for the current turn. The server checks it against the
    /// legal moves; whether it fills a forced space is the board's call.
    PlayMove {
        /// The match being played; the room refuses any other.
        match_id: u32,
        /// The tile, position and rotation to play.
        placement: Placement,
    },
    /// Finish the running game and score the board as it stands.
    EndGame {
        /// The match to end; the room refuses any other.
        match_id: u32,
    },
    /// Archive the finished match and return everyone to this room’s lobby.
    Rematch {
        /// The finished match. If another player already restarted it, the
        /// sender just gets the current state back.
        match_id: u32,
    },
    /// Pull the latest snapshot (e.g. after reconnect).
    RequestSnapshot,
    /// Change my display name and publish the updated roster.
    UpdateNickname {
        /// The new display name; the room trims it and refuses one that
        /// shows nothing or exceeds 24 characters.
        nickname: String,
    },
}

/// The server's response or broadcast to one or all peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostMessage {
    /// Permanent identity and the currently active invitation shortcut.
    RoomInfo {
        /// The room's permanent ID and current invitation code.
        room: RoomReference,
    },
    /// Server replies to `Hello`. If `rejected` is Some, the client should close.
    Welcome {
        /// The receiver's own participant ID; empty when `rejected`.
        your_peer_id: PeerId,
        /// The room's lobby; an empty one when `rejected`.
        lobby: LobbyState,
        /// Why the room turned this socket away; `None` once joined.
        rejected: Option<RejectCode>,
    },
    /// Lobby changed (someone joined, picked a color, left, etc.).
    LobbyUpdate {
        /// The new lobby state.
        lobby: LobbyState,
    },
    /// Lobby → playing transition. Clients should render the game view.
    GameStarted {
        /// The game as it stands; also sent on a mid-game join or `RequestSnapshot`.
        snapshot: GameSnapshot,
    },
    /// New authoritative state after a move or a roster change.
    /// `last_move` is the move as the server placed it.
    StateUpdate {
        /// The new game state.
        snapshot: GameSnapshot,
        /// The move just played; `None` after a roster change.
        last_move: Option<Move>,
    },
    /// Terminal state. After this the host stops publishing updates.
    GameEnded {
        /// Why the game ended, with its winners.
        reason: EndReason,
        /// The final game state.
        snapshot: GameSnapshot,
    },
    /// Per-peer rejection for the most recent intent.
    Error {
        /// Why the intent was refused; clients word their message from it.
        code: RejectCode,
        /// A short server-side explanation, such as "not your turn".
        message: String,
    },
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

/// Decode a message from a frame produced by [`encode`]. Fails on malformed
/// JSON or a message of another shape.
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
        let json = r#"{"PlayMove":{"match_id":0,"placement":{"tile":"RYYBRB","position":{"x":-1,"y":0},"rotation":"R60"}}}"#;

        let encoded = encode(&PeerMessage::PlayMove {
            match_id: 0,
            placement,
        });
        assert_eq!(String::from_utf8(encoded).unwrap(), json);
        let decoded: PeerMessage = decode(json.as_bytes()).unwrap();
        assert!(matches!(decoded, PeerMessage::PlayMove { placement: p, .. } if p == placement));
    }
}
