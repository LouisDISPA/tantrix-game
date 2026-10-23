//! Public session types shared by the browser transport and Dioxus UI.

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use tantrix_game::{Color, Move, Placement};
use tantrix_proto::{EndReason, GameSnapshot, LobbyState, PeerId, RejectCode};

#[cfg(any(target_arch = "wasm32", feature = "native", test))]
use tantrix_proto::HostMessage;
#[cfg(any(target_arch = "wasm32", feature = "native"))]
use tantrix_proto::PeerMessage;

/// Commands the UI sends into the session.
#[derive(Debug, Clone)]
pub enum SessionCommand {
    /// Claim/release a color in the lobby.
    PickColor(Option<Color>),
    /// Take a seat someone else holds (after the user confirmed).
    ReplaceSeat(Color),
    /// Transition the lobby into a running game.
    StartGame,
    /// Place a tile for the current turn.
    PlayMove(Placement),
    /// Finish the running game early.
    EndGame,
    /// Change the local player's display name.
    UpdateNickname(String),
    /// Leave the room. After this the session task exits.
    Leave,
}

#[cfg(any(target_arch = "wasm32", feature = "native"))]
impl SessionCommand {
    /// Leaving closes the transport locally; it has no wire message.
    pub(crate) fn into_peer_message(self) -> Option<PeerMessage> {
        Some(match self {
            Self::PickColor(color) => PeerMessage::PickColor { color },
            Self::ReplaceSeat(color) => PeerMessage::ReplaceSeat { color },
            Self::StartGame => PeerMessage::StartGame,
            Self::PlayMove(placement) => PeerMessage::PlayMove { placement },
            Self::EndGame => PeerMessage::EndGame,
            Self::UpdateNickname(nickname) => PeerMessage::UpdateNickname { nickname },
            Self::Leave => return None,
        })
    }
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

/// A welcome emits identity before lobby state; rejection emits the error
/// before disconnection. Transports handle these events' lifecycle effects.
#[cfg(any(target_arch = "wasm32", feature = "native", test))]
pub(crate) fn incoming_events(message: HostMessage) -> Vec<SessionEvent> {
    match message {
        HostMessage::Welcome {
            rejected: Some(code),
            ..
        } => {
            let reason = "The room rejected this connection.".to_string();
            vec![
                SessionEvent::Error {
                    code: Some(code),
                    message: reason.clone(),
                },
                SessionEvent::Disconnected { reason },
            ]
        }
        HostMessage::Welcome {
            your_peer_id,
            lobby,
            rejected: None,
        } => vec![
            SessionEvent::Connected {
                my_peer_id: your_peer_id,
            },
            SessionEvent::Lobby(lobby),
        ],
        HostMessage::LobbyUpdate { lobby } => vec![SessionEvent::Lobby(lobby)],
        HostMessage::GameStarted { snapshot } => vec![SessionEvent::GameStarted { snapshot }],
        HostMessage::StateUpdate {
            snapshot,
            last_move,
        } => {
            vec![SessionEvent::StateUpdate {
                snapshot,
                last_move,
            }]
        }
        HostMessage::GameEnded { reason, snapshot } => {
            vec![SessionEvent::GameEnded { reason, snapshot }]
        }
        HostMessage::Error { code, message } => {
            vec![SessionEvent::Error {
                code: Some(code),
                message,
            }]
        }
    }
}

/// Handle held by the UI. Drop to request shutdown.
pub struct SessionHandle {
    pub commands: UnboundedSender<SessionCommand>,
    pub events: UnboundedReceiver<SessionEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_emits_identity_before_lobby_and_rejection_never_connects() {
        let lobby = LobbyState {
            seats: Default::default(),
            unseated: vec![],
            participants: vec![],
        };
        let peer = PeerId::new("peer");
        let accepted = incoming_events(HostMessage::Welcome {
            your_peer_id: peer.clone(),
            lobby: lobby.clone(),
            rejected: None,
        });
        assert!(
            matches!(accepted.as_slice(), [SessionEvent::Connected { my_peer_id }, SessionEvent::Lobby(state)] if my_peer_id == &peer && state == &lobby)
        );
        let rejected = incoming_events(HostMessage::Welcome {
            your_peer_id: peer,
            lobby,
            rejected: Some(RejectCode::RoomNotFound),
        });
        assert!(
            matches!(rejected.as_slice(), [SessionEvent::Error { code: Some(RejectCode::RoomNotFound), message }, SessionEvent::Disconnected { reason }] if message == reason && !reason.is_empty())
        );
        let error = incoming_events(HostMessage::Error {
            code: RejectCode::InvalidMove,
            message: "Try another tile.".into(),
        });
        assert!(
            matches!(error.as_slice(), [SessionEvent::Error { code: Some(RejectCode::InvalidMove), message }] if message == "Try another tile.")
        );
    }
}
