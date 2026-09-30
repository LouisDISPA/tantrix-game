//! Public session types shared by the browser transport and Dioxus UI.

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use tantrix_game::{Color, Move};
use tantrix_proto::{EndReason, GameSnapshot, LobbyState, PeerId, RejectCode};

/// Commands the UI sends into the session.
#[derive(Debug, Clone)]
pub enum SessionCommand {
    /// Claim/release a color in the lobby.
    PickColor(Option<Color>),
    /// Take a seat someone else holds (after the user confirmed).
    ReplaceSeat(Color),
    /// Transition the lobby into a running game.
    StartGame,
    /// Submit a move for the current turn.
    PlayMove(Move),
    /// Pass because there are no legal moves.
    SkipTurn,
    /// Finish the running game early.
    EndGame,
    /// Change the local player's display name.
    UpdateNickname(String),
    /// Leave the room. After this the session task exits.
    Leave,
}

/// Events the session emits for the UI to render.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// The server accepted our Hello and assigned a public participant id.
    Connected { my_peer_id: PeerId },
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
    /// The server rejected an intent or reported an error.
    Error {
        code: Option<RejectCode>,
        message: String,
    },
    /// Connecting did not finish in time. The UI should offer a retry.
    ConnectTimeout,
    /// Fatal transport error. The session task has exited.
    Disconnected { reason: String },
}

/// Handle held by the UI. Drop to request shutdown.
pub struct SessionHandle {
    pub commands: UnboundedSender<SessionCommand>,
    pub events: UnboundedReceiver<SessionEvent>,
}
