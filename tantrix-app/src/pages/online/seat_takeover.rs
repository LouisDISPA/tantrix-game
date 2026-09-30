//! In-game panel for members without a seat: they watch, and can take over
//! any playing seat (e.g. from someone who disconnected) after confirming.
//! The newcomer inherits that seat's hand and turn.

use dioxus::prelude::*;
use tantrix_game::Color;
use tantrix_net::SessionCommand;

use crate::components::confirm_dialog::ConfirmDialog;
use crate::state::online_state::{
    queries, CommandSender, OnlinePhase, OnlineState, OnlineStateStoreExt,
};
use crate::svg::paths::color_name as color_slug;

#[derive(Clone, PartialEq)]
struct PlayingSeat {
    color: Color,
    nickname: String,
    online: bool,
}

#[component]
pub fn OnlineSeatTakeover(
    state: Store<OnlineState>,
    sender: Signal<Option<CommandSender>>,
) -> Element {
    let my_color = use_memo(move || queries::my_color_of(state));
    let seats = use_memo(move || {
        let phase = state.phase();
        let phase = phase.read();
        playing_seats(&phase)
    });
    // Local UI flag: which seat the confirmation dialog is about.
    let mut confirming = use_signal(|| Option::<PlayingSeat>::None);

    if my_color().is_some() || seats.read().is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "seat-takeover", role: "group", "aria-label": "Take a seat",
            span { class: "seat-takeover-title", "You're watching · take a seat:" }
            for seat in seats.read().iter().cloned() {
                button {
                    key: "{seat.color:?}",
                    class: "seat-takeover-btn color-{color_slug(seat.color)}",
                    class: if !seat.online { "offline" },
                    onclick: {
                        let seat = seat.clone();
                        move |_| confirming.set(Some(seat.clone()))
                    },
                    span { class: "seat-takeover-dot" }
                    "{seat.color} · {seat.nickname}"
                    if !seat.online {
                        span { class: "seat-takeover-offline", " (offline)" }
                    }
                }
            }
        }
        if let Some(seat) = confirming() {
            ConfirmDialog {
                title: format!("Take over {}?", seat.color),
                message: format!(
                    "You'll play {}'s tiles and turns from now on. {} will watch.",
                    seat.nickname, seat.nickname,
                ),
                confirm_label: "Take seat",
                on_confirm: move |_| {
                    confirming.set(None);
                    if let Some(s) = sender.read().as_ref() {
                        s.send(SessionCommand::ReplaceSeat(seat.color));
                    }
                },
                on_cancel: move |_| confirming.set(None),
            }
        }
    }
}

fn playing_seats(phase: &OnlinePhase) -> Vec<PlayingSeat> {
    let OnlinePhase::Playing { snapshot, .. } = phase else {
        return Vec::new();
    };
    snapshot
        .playing_colors
        .iter()
        .filter_map(|&color| {
            let seat = snapshot.seats[color].as_ref()?;
            let online = snapshot
                .participants
                .iter()
                .any(|p| p.peer_id == seat.peer_id && p.connected);
            Some(PlayingSeat {
                color,
                nickname: seat.nickname.clone(),
                online,
            })
        })
        .collect()
}
