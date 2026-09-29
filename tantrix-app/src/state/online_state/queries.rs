//! Derivations over [`OnlineState`] fields.
//!
//! These are **free functions** that take narrow borrows (the specific
//! fields they need) so a memoing caller subscribes only to those Store
//! fields — not to the whole state. The `*_of` wrappers take a
//! `Store<OnlineState>` directly and handle the Store field-accessor
//! borrow dance.

use dioxus::prelude::*;
use tantrix_game::Color;
use tantrix_proto::{Participant, ParticipantRole, PeerId};

use super::phase::OnlinePhase;
use super::state::{OnlineState, OnlineStateStoreExt};

/// Which color this peer currently occupies, if any.
pub fn my_color(phase: &OnlinePhase, me: Option<&PeerId>) -> Option<Color> {
    let me = me?;
    let seats = match phase {
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

/// Full roster — participants include spectators and disconnected
/// seated players. Empty during connecting / failed phases.
pub fn participants(phase: &OnlinePhase) -> &[Participant] {
    match phase {
        OnlinePhase::Lobby { lobby } => &lobby.participants,
        OnlinePhase::Playing { snapshot, .. } => &snapshot.participants,
        OnlinePhase::Ended { snapshot, .. } => &snapshot.participants,
        _ => &[],
    }
}

/// Am I a spectator (game started without me seated)?
pub fn is_spectator(phase: &OnlinePhase, me: Option<&PeerId>) -> bool {
    let Some(me) = me else { return false };
    participants(phase)
        .iter()
        .find(|p| &p.peer_id == me)
        .map(|p| matches!(p.role, ParticipantRole::Spectator))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Store-aware wrappers
// ---------------------------------------------------------------------------

pub fn my_color_of(state: Store<OnlineState>) -> Option<Color> {
    let phase = state.phase();
    let phase = phase.read();
    let me = state.my_peer_id();
    let me = me.read();
    my_color(&phase, me.as_ref())
}

pub fn participant_count_of(state: Store<OnlineState>) -> usize {
    let phase = state.phase();
    let phase = phase.read();
    participants(&phase).len()
}

pub fn disconnected_count_of(state: Store<OnlineState>) -> usize {
    let phase = state.phase();
    let phase = phase.read();
    participants(&phase).iter().filter(|p| !p.connected).count()
}

pub fn is_spectator_of(state: Store<OnlineState>) -> bool {
    let phase = state.phase();
    let phase = phase.read();
    let me = state.my_peer_id();
    let me = me.read();
    is_spectator(&phase, me.as_ref())
}
