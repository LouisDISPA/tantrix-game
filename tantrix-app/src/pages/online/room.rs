//! `/online/:code` — root component that owns the session.
//!
//! Creates the `Store<OnlineState>` and the command-sender signal locally
//! and passes them down as props to each child screen. No context.
//!
//! The session itself lives in [`OnlineRoomSession`], keyed by a retry
//! counter: bumping the counter unmounts the old session (dropping its
//! WebSocket and cancelling its tasks) and mounts a fresh one, which is
//! what the "Retry" buttons on the connecting and failed screens do.

use dioxus::prelude::*;

use crate::pages::online::driver::spawn_session;
use crate::pages::online::game::OnlineGameView;
use crate::pages::online::lobby::OnlineLobbyView;
use crate::pages::online::screens::{ConnectingView, FailedView};
use crate::state::nickname;
use crate::state::online_state::{OnlinePhase, OnlineState, OnlineStateStoreExt};
use crate::state::room_connection;

#[component]
pub fn OnlineRoom(code: String) -> Element {
    let mut attempt = use_signal(|| 0u32);
    let create = use_hook({
        let code = code.clone();
        move || room_connection::take_create(&code)
    });

    // Keys only take effect between siblings: Dioxus never remounts a lone
    // keyed child, so a bare `OnlineRoomSession { key: .. }` would be reused
    // as-is when the counter changes. Rendering it as a one-item keyed list
    // makes a new key drop the old session and mount a fresh one.
    rsx! {
        for attempt_no in [attempt()] {
            OnlineRoomSession {
                key: "{code}-{attempt_no}",
                code: code.clone(),
                create: create && attempt_no == 0,
                on_retry: move |_| {
                    log::info!("online: retrying (attempt {})", attempt() + 2);
                    attempt += 1;
                },
            }
        }
    }
}

#[component]
fn OnlineRoomSession(code: String, create: bool, on_retry: EventHandler<()>) -> Element {
    let initial_nickname = nickname::load_or_generate();

    let state = use_store(|| OnlineState::new(code.clone()));
    let sender = use_signal(|| None);

    use_hook(|| {
        spawn_session(
            state,
            sender,
            code.clone(),
            create,
            initial_nickname.clone(),
        );
    });

    let phase = state.phase();
    let phase = phase.read().clone();
    match phase {
        OnlinePhase::Connecting => rsx! { ConnectingView { state, on_retry } },
        OnlinePhase::Lobby { .. } => rsx! { OnlineLobbyView { state, sender } },
        OnlinePhase::Playing { .. } => rsx! { OnlineGameView { state, sender } },
        OnlinePhase::Ended { .. } => rsx! { OnlineGameView { state, sender } },
        OnlinePhase::Failed { reason } => rsx! { FailedView { reason, on_retry } },
    }
}
