//! Public session types shared by host and client implementations.

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use tantrix_game::{Color, Move};
use tantrix_proto::{EndReason, GameSnapshot, LobbyState, PeerId, RejectCode};

/// Which role this peer is playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRole {
    Host,
    Client,
}

/// Commands the UI sends into the session.
#[derive(Debug, Clone)]
pub enum SessionCommand {
    /// Claim/release a color in the lobby.
    PickColor(Option<Color>),
    /// Host-only: transition the lobby into a running game.
    StartGame,
    /// Submit a move for the current turn.
    PlayMove(Move),
    /// Pass because there are no legal moves.
    SkipTurn,
    /// Leave the room. After this the session task exits.
    Leave,
}

/// Events the session emits for the UI to render.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// Signaling succeeded and the socket is live. For clients this fires
    /// once a `Welcome` arrives; for hosts it fires as soon as a peer id is
    /// assigned.
    Connected {
        role: SessionRole,
        my_peer_id: PeerId,
        host_peer_id: PeerId,
    },
    /// Lobby state changed.
    Lobby(LobbyState),
    /// Lobby → game transition.
    GameStarted { snapshot: GameSnapshot },
    /// New authoritative state after a move was applied.
    StateUpdate {
        snapshot: GameSnapshot,
        last_move: Option<Move>,
    },
    /// Terminal state.
    GameEnded {
        reason: EndReason,
        snapshot: GameSnapshot,
    },
    /// The host rejected an intent or reported an error.
    Error {
        code: Option<RejectCode>,
        message: String,
    },
    /// Fatal transport error. The session task has exited.
    Disconnected { reason: String },
}

/// Handle held by the UI. Drop to request shutdown.
pub struct SessionHandle {
    pub commands: UnboundedSender<SessionCommand>,
    pub events: UnboundedReceiver<SessionEvent>,
}
