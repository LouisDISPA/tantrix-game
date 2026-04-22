//! Reactive state for an online room.
//!
//! Held as a `Signal<OnlineState>` in the online-game pages. The driver task
//! (spawned in [`crate::pages::online::driver`]) pumps
//! [`tantrix_net::SessionEvent`]s into this signal.

use dioxus::prelude::*;
use futures_channel::mpsc::UnboundedSender;
use tantrix_game::{Color, Move};
use tantrix_net::{SessionCommand, SessionRole};
use tantrix_proto::{EndReason, GameSnapshot, LobbyState, PeerId, RejectCode};

/// Top-level lifecycle of the online session.
#[derive(Debug, Clone)]
pub enum OnlinePhase {
    /// Matchbox socket is being opened; no peer id yet.
    Connecting,
    /// Socket is live, we're in the lobby picking colors.
    Lobby { lobby: LobbyState },
    /// Game is running.
    Playing {
        snapshot: GameSnapshot,
        last_move: Option<Move>,
    },
    /// Terminal state. `reason` is retained for a future end-of-game
    /// screen even though the current HUD only shows the snapshot.
    Ended {
        #[allow(dead_code)]
        reason: EndReason,
        snapshot: GameSnapshot,
    },
    /// Fatal error; the session task has exited.
    Failed { reason: String },
}

#[derive(Debug, Clone)]
pub struct OnlineState {
    pub room_code: String,
    pub role: SessionRole,
    /// The nickname the user entered; kept so the UI can redisplay it if we
    /// ever add a "return to lobby" path.
    #[allow(dead_code)]
    pub nickname: String,
    pub my_peer_id: Option<PeerId>,
    pub host_peer_id: Option<PeerId>,
    pub phase: OnlinePhase,
    /// Last error shown to the user, cleared on next successful update.
    pub last_error: Option<String>,
}

impl OnlineState {
    pub fn new(room_code: String, role: SessionRole, nickname: String) -> Self {
        Self {
            room_code,
            role,
            nickname,
            my_peer_id: None,
            host_peer_id: None,
            phase: OnlinePhase::Connecting,
            last_error: None,
        }
    }

    /// Which color this peer currently occupies, if any.
    pub fn my_color(&self) -> Option<Color> {
        let me = self.my_peer_id.as_ref()?;
        let seats = match &self.phase {
            OnlinePhase::Lobby { lobby } => &lobby.seats,
            OnlinePhase::Playing { snapshot, .. } => &snapshot.seats,
            OnlinePhase::Ended { snapshot, .. } => &snapshot.seats,
            _ => return None,
        };
        for (c, slot) in seats.iter() {
            if let Some(seat) = slot {
                if &seat.peer_id == me {
                    return Some(c);
                }
            }
        }
        None
    }

    pub fn is_host(&self) -> bool {
        self.role == SessionRole::Host
    }
}

/// Context injected into the online pages.
#[derive(Clone, Copy)]
pub struct OnlineCtx {
    pub state: Signal<OnlineState>,
    pub sender: Signal<Option<CommandSender>>,
}

/// A cloneable handle to the session's command sender. Boxed separately so
/// the `Signal` type parameter stays small and `Copy`.
#[derive(Clone)]
pub struct CommandSender(pub UnboundedSender<SessionCommand>);

impl CommandSender {
    pub fn send(&self, cmd: SessionCommand) {
        let _ = self.0.unbounded_send(cmd);
    }
}

pub fn reject_message(code: &RejectCode) -> &'static str {
    match code {
        RejectCode::LobbyFull => "The room is already full.",
        RejectCode::ColorTaken => "That color is taken.",
        RejectCode::IncompatibleVersion => "The host is running a different game version.",
        RejectCode::NotYourTurn => "It's not your turn.",
        RejectCode::InvalidMove => "The host rejected that move.",
        RejectCode::GameInProgress => "This room's game is already in progress.",
    }
}
