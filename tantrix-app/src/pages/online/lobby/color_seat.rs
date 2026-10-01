//! Lobby color-seat button — wraps [`ColorChip`] with the store
//! subscription + `PickColor` dispatch. A seat someone else holds can be
//! taken over after a confirmation dialog (`ReplaceSeat`).

use dioxus::prelude::*;
use tantrix_game::Color;
use tantrix_net::SessionCommand;
use tantrix_proto::Seat;

use crate::components::color_chip::{ColorChip, ColorChipState};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::state::online_state::{
    queries, CommandSender, OnlinePhase, OnlineState, OnlineStateStoreExt,
};

#[component]
pub fn LobbyColorSeat(
    color: Color,
    state: Store<OnlineState>,
    sender: Signal<Option<CommandSender>>,
) -> Element {
    let seat = use_memo(move || {
        let phase = state.phase();
        let phase = phase.read();
        seat_for(&phase, color)
    });
    let holder_online = use_memo(move || {
        let phase = state.phase();
        let phase = phase.read();
        seat_holder_online(&phase, color)
    });
    let my_color = use_memo(move || queries::my_color_of(state));
    let chip_state = use_memo(move || {
        let is_mine = my_color() == Some(color);
        if is_mine {
            ColorChipState::Mine
        } else if seat.read().is_some() && holder_online() {
            ColorChipState::TakenByOther
        } else if seat.read().is_some() {
            ColorChipState::Offline
        } else {
            ColorChipState::Free
        }
    });
    let seat_label = use_memo(move || seat.read().as_ref().map(|s: &Seat| s.nickname.clone()));
    // Local UI flag: the "take this seat?" dialog is open.
    let mut confirming = use_signal(|| false);

    let send = move |command: SessionCommand| {
        if let Some(s) = sender.read().as_ref() {
            s.send(command);
        }
    };
    let on_click = move |_| match chip_state() {
        ColorChipState::Mine => send(SessionCommand::PickColor(None)),
        ColorChipState::Free => send(SessionCommand::PickColor(Some(color))),
        ColorChipState::TakenByOther | ColorChipState::Offline => confirming.set(true),
    };

    rsx! {
        ColorChip {
            color,
            state: chip_state(),
            seat_label,
            on_click,
        }
        if confirming() {
            ConfirmDialog {
                title: format!("Take the {color} seat?"),
                message: replace_message(seat_label().as_deref(), holder_online()),
                confirm_label: "Take seat",
                on_confirm: move |_| {
                    confirming.set(false);
                    send(SessionCommand::ReplaceSeat(color));
                },
                on_cancel: move |_| confirming.set(false),
            }
        }
    }
}

fn replace_message(holder: Option<&str>, online: bool) -> String {
    let holder = holder.unwrap_or("Another player");
    if online {
        format!("{holder} is sitting here. They'll lose the seat and can pick another one.")
    } else {
        format!("{holder} is offline. They'll lose the seat and leave the room.")
    }
}

/// Whether the player holding `color` has a live connection.
fn seat_holder_online(phase: &OnlinePhase, color: Color) -> bool {
    let OnlinePhase::Lobby { lobby } = phase else {
        return false;
    };
    let Some(seat) = &lobby.seats[color] else {
        return false;
    };
    lobby
        .participants
        .iter()
        .any(|p| p.peer_id == seat.peer_id && p.connected)
}

fn seat_for(phase: &OnlinePhase, color: Color) -> Option<Seat> {
    match phase {
        OnlinePhase::Lobby { lobby } => lobby.seats[color].clone(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use tantrix_game::Color::{Blue, Green, Red};

    use super::{replace_message, seat_for, seat_holder_online};
    use crate::state::online_state::test_rooms::{lobby, seat, snapshot};
    use crate::state::online_state::OnlinePhase;

    #[test]
    fn each_seat_knows_its_holder_and_whether_they_are_online() {
        let seated = [(Red, "Ann", true), (Blue, "Bob", false)];
        let phase = OnlinePhase::Lobby {
            lobby: lobby(&seated, &[]),
        };
        assert_eq!(seat_for(&phase, Red), Some(seat("Ann")));
        assert!(seat_holder_online(&phase, Red));
        assert_eq!(seat_for(&phase, Blue), Some(seat("Bob")));
        assert!(!seat_holder_online(&phase, Blue));
        assert_eq!(seat_for(&phase, Green), None);
        assert!(!seat_holder_online(&phase, Green));

        // The lobby's seats only: a running game has its own panel.
        let playing = OnlinePhase::Playing {
            snapshot: snapshot(&seated, &[]),
            last_move: None,
        };
        assert_eq!(seat_for(&playing, Red), None);
        assert!(!seat_holder_online(&playing, Red));
    }

    #[test]
    fn taking_a_seat_says_what_happens_to_its_holder() {
        assert_eq!(
            replace_message(Some("Ann"), true),
            "Ann is sitting here. They'll lose the seat and can pick another one."
        );
        assert_eq!(
            replace_message(None, false),
            "Another player is offline. They'll lose the seat and leave the room."
        );
    }
}
