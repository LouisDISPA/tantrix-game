//! Lobby screen: shows joined peers, lets each player claim a color and
//! edit their nickname, and lets the host start the game.

mod color_seat;
mod nickname_editor;
mod participants;
mod share_link;

use dioxus::prelude::*;
use tantrix_game::Color;
use tantrix_net::{SessionCommand, SessionRole};

use crate::components::alert_banner::AlertBanner;
use crate::components::room_code_badge::RoomCodeBadge;
use crate::components::share_chip::ShareChip;
use crate::state::online_state::{
    queries, reject_message, CommandSender, OnlinePhase, OnlineState, OnlineStateStoreExt,
};

use color_seat::LobbyColorSeat;
use nickname_editor::LobbyNicknameEditor;
use share_link::share_link;

pub(super) use participants::LobbyParticipants;

const COLORS: [Color; 4] = [Color::Red, Color::Blue, Color::Yellow, Color::Green];

#[component]
pub fn OnlineLobbyView(
    state: Store<OnlineState>,
    sender: Signal<Option<CommandSender>>,
) -> Element {
    let is_host = use_memo(move || *state.role().read() == SessionRole::Host);
    let my_color = use_memo(move || queries::my_color_of(state));
    let seated_count = use_memo(move || {
        let phase = state.phase();
        let phase = phase.read();
        seats_filled(&phase)
    });
    let connected_seated_count = use_memo(move || {
        let phase = state.phase();
        let phase = phase.read();
        connected_seats_filled(&phase)
    });
    let can_start =
        use_memo(move || seated_count() >= 2 && connected_seated_count() == seated_count());
    let error_message = use_memo(move || {
        let last = state.last_error();
        let last = last.read();
        last.as_ref().map(reject_message)
    });
    let share_url = use_memo(move || share_link(&state.room_code().read()));

    // Gate on actually being in the Lobby phase — otherwise render nothing
    // (the router should already have picked a different screen).
    let phase_ok = {
        let phase = state.phase();
        let phase = phase.read();
        matches!(&*phase, OnlinePhase::Lobby { .. })
    };
    if !phase_ok {
        return rsx! {};
    }

    rsx! {
        div { class: "title-screen lobby-screen",
            div { class: "title-content lobby-content",
                div { class: "lobby-header",
                    h1 { class: "title-logo", "LOBBY" }
                    RoomCodeBadge { code: state.room_code() }
                    ShareChip { url: share_url }
                }

                section { class: "lobby-section narrow",
                    h2 { class: "lobby-section-title", "Your nickname" }
                    LobbyNicknameEditor { state, sender }
                }

                section { class: "lobby-section",
                    h2 { class: "lobby-section-title", "Players" }
                    LobbyParticipants { state, sender }
                }

                section { class: "lobby-section",
                    h2 { class: "lobby-section-title", "Pick your color" }
                    div { class: "lobby-color-row",
                        for color in COLORS.iter().copied() {
                            LobbyColorSeat { key: "{color:?}", color, state, sender }
                        }
                    }
                    if my_color().is_some() {
                        button {
                            class: "lobby-release-btn",
                            onclick: move |_| {
                                if let Some(s) = sender.read().as_ref() {
                                    s.send(SessionCommand::PickColor(None));
                                }
                            },
                            "Release my seat"
                        }
                    }
                }

                AlertBanner { message: error_message }

                div { class: "lobby-actions",
                    if is_host() {
                        button {
                            class: "title-btn title-btn-primary",
                            disabled: !can_start(),
                            onclick: move |_| {
                                if can_start() {
                                    if let Some(s) = sender.read().as_ref() {
                                        s.send(SessionCommand::StartGame);
                                    }
                                }
                            },
                            div { class: "title-btn-inner",
                                if can_start() { "Start game" } else if seated_count() >= 2 { "Waiting for reconnection…" } else { "Waiting for players…" }
                            }
                        }
                    } else {
                        p { class: "lobby-waiting", "Waiting for the host to start the game…" }
                    }
                }
            }
        }
    }
}

fn seats_filled(phase: &OnlinePhase) -> usize {
    match phase {
        OnlinePhase::Lobby { lobby } => lobby.seats.values().filter(|s| s.is_some()).count(),
        _ => 0,
    }
}

fn connected_seats_filled(phase: &OnlinePhase) -> usize {
    match phase {
        OnlinePhase::Lobby { lobby } => lobby
            .seats
            .values()
            .filter_map(|s| s.as_ref())
            .filter(|seat| {
                lobby
                    .participants
                    .iter()
                    .any(|p| p.peer_id == seat.peer_id && p.connected)
            })
            .count(),
        _ => 0,
    }
}
