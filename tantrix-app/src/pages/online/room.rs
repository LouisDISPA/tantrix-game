//! `/online/:code/:role` — root component that owns the session.
//!
//! Creates the `Store<OnlineState>` and the command-sender signal locally
//! and passes them down as props to each child screen. No context.

use dioxus::prelude::*;

use crate::pages::online::driver::spawn_session;
use crate::pages::online::game::OnlineGameView;
use crate::pages::online::lobby::OnlineLobbyView;
use crate::pages::online::screens::{ConnectingView, FailedView};
use crate::route::RoleParam;
use crate::state::nickname;
use crate::state::online_state::{OnlinePhase, OnlineState, OnlineStateStoreExt};

#[component]
pub fn OnlineRoom(code: String, role: RoleParam) -> Element {
    let session_role = role.as_session_role();
    let initial_nickname = nickname::load_or_generate();

    let state = use_store(|| OnlineState::new(code.clone(), session_role));
    let sender = use_signal(|| None);

    use_hook(|| {
        spawn_session(
            state,
            sender,
            code.clone(),
            session_role,
            initial_nickname.clone(),
        );
    });

    let phase = state.phase();
    let phase = phase.read().clone();
    match phase {
        OnlinePhase::Connecting => rsx! { ConnectingView { state } },
        OnlinePhase::Lobby { .. } => rsx! { OnlineLobbyView { state, sender } },
        OnlinePhase::Playing { .. } => rsx! { OnlineGameView { state, sender } },
        OnlinePhase::Ended { .. } => rsx! { OnlineGameView { state, sender } },
        OnlinePhase::Failed { reason } => rsx! { FailedView { reason } },
    }
}
