//! Lobby screen: choose a color, share the room, and start with 2–4 players.
//! Any number of people can wait here; the four seats are taken and released
//! freely, and anyone still unseated at the start watches the game.

mod color_seat;
mod nickname_editor;
mod share_link;

use dioxus::prelude::*;
use tantrix_game::Color;
use tantrix_net::SessionCommand;

use crate::components::alert_banner::AlertBanner;
use crate::components::menu_button::MenuButton;
use crate::components::menu_card::MenuCard;
use crate::components::menu_screen::MenuScreen;
use crate::components::room_code_badge::RoomCodeBadge;
use crate::components::share_chip::ShareChip;
use crate::route::Route;
use crate::state::online_state::{
    queries, reject_message, CommandSender, OnlinePhase, OnlineState, OnlineStateStoreExt,
};

use color_seat::LobbyColorSeat;
use nickname_editor::LobbyNicknameEditor;
use share_link::share_link;

const COLORS: [Color; 4] = [Color::Red, Color::Blue, Color::Yellow, Color::Green];

#[component]
pub fn OnlineLobbyView(
    state: Store<OnlineState>,
    sender: Signal<Option<CommandSender>>,
) -> Element {
    let nav = navigator();
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
    let can_start = use_memo(move || {
        my_color().is_some() && seated_count() >= 2 && connected_seated_count() == seated_count()
    });
    let error_message = use_memo(move || {
        let last = state.last_error();
        let last = last.read();
        last.as_ref().map(reject_message)
    });
    let share_url = use_memo(move || share_link(&state.room_code().read()));
    let waiting = use_memo(move || {
        let phase = state.phase();
        let phase = phase.read();
        waiting_names(&phase)
    });

    // Gate on actually being in the Lobby phase — otherwise render nothing
    // (the router should already have picked a different screen). A memo,
    // so a lobby update re-renders this view only through the memos above
    // whose values it changes.
    let in_lobby = use_memo(move || {
        let phase = state.phase();
        let phase = phase.read();
        matches!(&*phase, OnlinePhase::Lobby { .. })
    });
    if !in_lobby() {
        return rsx! {};
    }

    rsx! {
        MenuScreen { scroll: true,
            MenuCard {
                title: "Lobby",
                subtitle: "Share the room and pick a color",
                wide: true,
                back: move |_| { let _ = nav.push(Route::OnlineEntry {}); },
                div { class: "lobby-header",
                    RoomCodeBadge { code: state.room_code() }
                    ShareChip { url: share_url }
                }

                section { class: "lobby-section narrow",
                    h2 { class: "lobby-section-title", "Your nickname" }
                    LobbyNicknameEditor { state, sender }
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
                    if !waiting.read().is_empty() {
                        p { class: "lobby-waiting lobby-unseated", "Waiting for a seat: {waiting}" }
                    }
                }

                AlertBanner { message: error_message }

                div { class: "lobby-actions",
                    MenuButton {
                        disabled: !can_start(),
                        on_click: move |_| {
                            if can_start() {
                                if let Some(s) = sender.read().as_ref() {
                                    s.send(SessionCommand::StartGame);
                                }
                            }
                        },
                        if can_start() { "Start game" } else if my_color().is_none() { "Pick a color" } else if seated_count() >= 2 { "Waiting for reconnection…" } else { "Waiting for players…" }
                    }
                }
            }
        }
    }
}

/// Members without a color, joined for display ("Ann, Bob").
fn waiting_names(phase: &OnlinePhase) -> String {
    match phase {
        OnlinePhase::Lobby { lobby } => lobby
            .unseated
            .iter()
            .map(|seat| seat.nickname.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
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
