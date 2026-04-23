//! Store-aware participants list. Shared by the lobby and the in-game
//! drawer.
//!
//! * `LobbyParticipants` — the `<ul>` shell. Subscribes only to the
//!   participant count; individual rows re-render independently.
//! * `LobbyParticipantRow` — one row, keyed by index. Subscribes only to
//!   its own slot, derives display data, and wires the kick callback.
//!   Renders the presentational [`ParticipantRow`] leaf.

use dioxus::prelude::*;
use tantrix_net::{SessionCommand, SessionRole};
use tantrix_proto::{Participant, ParticipantRole};

use crate::components::participant_row::{ParticipantRow, ParticipantRowData, SeatDot};
use crate::state::online_state::{queries, CommandSender, OnlineState, OnlineStateStoreExt};
use crate::svg::paths::color_name as color_slug;

#[component]
pub fn LobbyParticipants(
    state: Store<OnlineState>,
    sender: Signal<Option<CommandSender>>,
) -> Element {
    let len = use_memo(move || queries::participant_count_of(state));

    rsx! {
        ul { class: "lobby-peer-list",
            for index in 0..len() {
                LobbyParticipantRow {
                    key: "{index}",
                    index,
                    state,
                    sender,
                }
            }
        }
    }
}

#[component]
pub fn LobbyParticipantRow(
    index: usize,
    state: Store<OnlineState>,
    sender: Signal<Option<CommandSender>>,
) -> Element {
    let participant = use_memo(move || queries::participant_at_of(state, index));

    // Early-out if the index went out of range between renders.
    let Some(p) = participant.read().clone() else {
        return rsx! {};
    };

    let is_me = use_memo({
        let peer_id = p.peer_id.clone();
        move || state.my_peer_id().read().as_ref() == Some(&peer_id)
    });
    let is_host_view = use_memo(move || *state.role().read() == SessionRole::Host);
    let is_host_row = matches!(p.role, ParticipantRole::Host);

    let row_data = ParticipantRowData {
        name: p.nickname.clone(),
        dot: SeatDot(dot_slug(&p)),
        role_tag: role_label(&p, is_host_row),
        is_you: is_me(),
        offline: !p.connected,
    };

    let can_kick = is_host_view() && !is_host_row && !is_me();
    let on_kick = can_kick.then(|| {
        let target = p.peer_id.clone();
        EventHandler::new(move |_| {
            if let Some(s) = sender.read().as_ref() {
                s.send(SessionCommand::Kick(target.clone()));
            }
        })
    });

    rsx! { ParticipantRow { row: row_data, on_kick } }
}

fn dot_slug(p: &Participant) -> &'static str {
    match p.seated_color {
        Some(c) => color_slug(c),
        None => "unseated",
    }
}

/// If they hold a seat, no textual role tag is needed — the color dot
/// conveys it. "Host" is always labelled (can be combined with a seat).
fn role_label(p: &Participant, is_host_row: bool) -> Option<&'static str> {
    if is_host_row {
        return Some("host");
    }
    if p.seated_color.is_some() {
        return None;
    }
    match &p.role {
        ParticipantRole::Spectator => Some("spectator"),
        ParticipantRole::Unseated => Some("picking…"),
        _ => None,
    }
}
