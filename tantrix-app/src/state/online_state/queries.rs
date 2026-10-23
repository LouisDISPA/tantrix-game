//! Derivations over [`OnlineState`] fields.
//!
//! These are **free functions** that take narrow borrows (the specific
//! fields they need) so a memoing caller subscribes only to those Store
//! fields — not to the whole state. The `*_of` wrappers take a
//! `Store<OnlineState>` directly and handle the Store field-accessor
//! borrow dance.

use dioxus::prelude::*;
use tantrix_game::Color;
use tantrix_proto::PeerId;

use super::phase::OnlinePhase;
use super::state::{OnlineState, OnlineStateStoreExt};

pub fn match_id_of(state: Store<OnlineState>) -> u32 {
    match &*state.phase().read() {
        OnlinePhase::Lobby { lobby } => lobby.match_id,
        OnlinePhase::Playing { snapshot, .. } | OnlinePhase::Ended { snapshot, .. } => {
            snapshot.match_id
        }
        _ => 0,
    }
}

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

#[cfg(test)]
mod tests {
    use tantrix_game::Color::{Blue, Red};
    use tantrix_proto::{EndReason, PeerId};

    use super::my_color;
    use crate::state::online_state::test_rooms::{lobby, snapshot};
    use crate::state::online_state::OnlinePhase;

    #[test]
    fn this_peer_plays_the_seat_it_sits_at() {
        let seated = [(Red, "Ann", true), (Blue, "Bob", true)];
        let ann = PeerId::new("Ann");
        let bob = PeerId::new("Bob");
        let lobby = OnlinePhase::Lobby {
            lobby: lobby(&seated, &["Cy"]),
        };
        let playing = OnlinePhase::Playing {
            snapshot: snapshot(&seated, &["Cy"]),
            last_move: None,
        };
        let ended = OnlinePhase::Ended {
            reason: EndReason::Completed { winners: vec![Red] },
            snapshot: snapshot(&seated, &["Cy"]),
        };
        for phase in [lobby, playing, ended] {
            assert_eq!(my_color(&phase, Some(&ann)), Some(Red), "{phase:?}");
            assert_eq!(my_color(&phase, Some(&bob)), Some(Blue), "{phase:?}");
            // Cy waits or watches; before the welcome, nobody is anybody.
            assert_eq!(my_color(&phase, Some(&PeerId::new("Cy"))), None);
            assert_eq!(my_color(&phase, None), None);
        }
        assert_eq!(my_color(&OnlinePhase::Connecting, Some(&ann)), None);
    }
}
