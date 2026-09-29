//! Lobby color-seat button — wraps [`ColorChip`] with the store
//! subscription + `PickColor` dispatch.

use dioxus::prelude::*;
use tantrix_game::Color;
use tantrix_net::SessionCommand;
use tantrix_proto::Seat;

use crate::components::color_chip::{ColorChip, ColorChipState};
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
    let my_color = use_memo(move || queries::my_color_of(state));
    let chip_state = use_memo(move || {
        let is_mine = my_color() == Some(color);
        if is_mine {
            ColorChipState::Mine
        } else if seat.read().is_some() {
            ColorChipState::TakenByOther
        } else {
            ColorChipState::Free
        }
    });
    let seat_label = use_memo(move || seat.read().as_ref().map(|s: &Seat| s.nickname.clone()));

    let on_click = move |_| {
        let Some(s) = sender.read().as_ref().cloned() else {
            return;
        };
        let pick = if chip_state() == ColorChipState::Mine {
            None
        } else {
            Some(color)
        };
        s.send(SessionCommand::PickColor(pick));
    };

    rsx! {
        ColorChip {
            color,
            state: chip_state(),
            seat_label,
            on_click,
        }
    }
}

fn seat_for(phase: &OnlinePhase, color: Color) -> Option<Seat> {
    match phase {
        OnlinePhase::Lobby { lobby } => lobby.seats[color].clone(),
        _ => None,
    }
}
