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
                            class: "button button-small button-secondary lobby-release-btn",
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

#[cfg(test)]
mod tests {
    use tantrix_game::Color::{Blue, Green, Red};

    use super::{connected_seats_filled, seats_filled, waiting_names};
    use crate::state::online_state::test_rooms::{lobby, snapshot};
    use crate::state::online_state::{FailureReason, OnlinePhase};

    #[test]
    fn the_lobby_counts_its_seats_and_who_holds_them_live() {
        let phase = OnlinePhase::Lobby {
            lobby: lobby(
                &[
                    (Red, "Ann", true),
                    (Blue, "Bob", false),
                    (Green, "Cy", true),
                ],
                &["Dee", "Eve"],
            ),
        };
        assert_eq!(seats_filled(&phase), 3);
        // Bob dropped: the game waits for him to reconnect.
        assert_eq!(connected_seats_filled(&phase), 2);
        assert_eq!(waiting_names(&phase), "Dee, Eve");
    }

    #[test]
    fn an_empty_lobby_has_no_one_to_count_or_name() {
        let phase = OnlinePhase::Lobby {
            lobby: lobby(&[], &[]),
        };
        assert_eq!(seats_filled(&phase), 0);
        assert_eq!(connected_seats_filled(&phase), 0);
        assert_eq!(waiting_names(&phase), "");
    }

    #[test]
    fn only_the_lobby_is_counted() {
        let playing = OnlinePhase::Playing {
            snapshot: snapshot(&[(Red, "Ann", true), (Blue, "Bob", true)], &["Cy"]),
            last_move: None,
        };
        let failed = OnlinePhase::Failed {
            reason: FailureReason::Timeout,
        };
        for phase in [OnlinePhase::Connecting, playing, failed] {
            assert_eq!(seats_filled(&phase), 0, "{phase:?}");
            assert_eq!(connected_seats_filled(&phase), 0, "{phase:?}");
            assert_eq!(waiting_names(&phase), "", "{phase:?}");
        }
    }
}
