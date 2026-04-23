//! Floating drawer shown in-game with the full participants list (seated +
//! spectators) and kick controls for the host.

use dioxus::prelude::*;

use crate::pages::online::lobby::LobbyParticipants;
use crate::state::online_state::{queries, CommandSender, OnlineState};

#[component]
pub fn ParticipantsDrawer(
    state: Store<OnlineState>,
    sender: Signal<Option<CommandSender>>,
) -> Element {
    let total = use_memo(move || queries::participant_count_of(state));
    let disconnected = use_memo(move || queries::disconnected_count_of(state));
    let mut open = use_signal(|| false);

    if total() == 0 {
        return rsx! {};
    }

    rsx! {
        div { class: "online-participants-drawer",
            button {
                class: "online-participants-toggle",
                onclick: move |_| {
                    let v = !*open.read();
                    open.set(v);
                },
                "Players ({total})"
                if disconnected() > 0 {
                    " · {disconnected} offline"
                }
            }
            if open() {
                div { class: "online-participants-body",
                    LobbyParticipants { state, sender }
                }
            }
        }
    }
}
