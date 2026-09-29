//! Store-aware participants list. Shared by the lobby and the in-game
//! drawer.
//!
//! * `LobbyParticipants` — the `<ul>` shell.
//! * `LobbyParticipantRow` — one row, keyed by stable participant token.

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
    let participants = use_memo(move || {
        let phase = state.phase();
        let phase = phase.read();
        queries::participants(&phase).to_vec()
    });

    rsx! {
        ul { class: "lobby-peer-list",
            for participant in participants() {
                {
                    let key = participant.token.as_str().to_string();
                    rsx! {
                LobbyParticipantRow {
                    key: "{key}",
                    participant,
                    state,
                    sender,
                }
                    }
                }
            }
        }
    }
}

#[component]
pub fn LobbyParticipantRow(
    participant: Participant,
    state: Store<OnlineState>,
    sender: Signal<Option<CommandSender>>,
) -> Element {
    let p = participant;

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
