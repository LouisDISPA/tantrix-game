//! `/online/:code/:role/:nickname` — root component that owns the session.
//!
//! Renders the Lobby or the OnlineGame depending on the current phase.

use dioxus::prelude::*;
use tantrix_net::SessionRole;

use crate::pages::online::driver::spawn_session;
use crate::pages::online::game::OnlineGameView;
use crate::pages::online::lobby::OnlineLobbyView;
use crate::route::RoleParam;
use crate::state::online_state::{OnlineCtx, OnlinePhase, OnlineState};

#[component]
pub fn OnlineRoom(code: String, nickname: String, role: RoleParam) -> Element {
    let session_role = role.as_session_role();

    let state = use_signal(|| OnlineState::new(code.clone(), session_role, nickname.clone()));
    let sender = use_signal(|| None);
    let ctx = OnlineCtx { state, sender };
    use_context_provider(|| ctx);

    // Kick off the session exactly once per mount.
    use_hook(|| {
        spawn_session(ctx, code.clone(), session_role, nickname.clone());
    });

    let phase = state.read().phase.clone();
    match phase {
        OnlinePhase::Connecting => rsx! { ConnectingView {} },
        OnlinePhase::Lobby { .. } => rsx! { OnlineLobbyView {} },
        OnlinePhase::Playing { .. } => rsx! { OnlineGameView {} },
        OnlinePhase::Ended { .. } => rsx! { OnlineGameView {} },
        OnlinePhase::Failed { reason } => rsx! { FailedView { reason } },
    }
}

#[component]
fn ConnectingView() -> Element {
    let ctx = use_context::<OnlineCtx>();
    let code = ctx.state.read().room_code.clone();
    let label = match ctx.state.read().role {
        SessionRole::Host => "Creating room…",
        SessionRole::Client => "Joining room…",
    };
    rsx! {
        div { class: "title-screen",
            div { class: "title-content",
                h1 { class: "title-logo", "{label}" }
                p { class: "title-subtitle", "Room {code}" }
            }
        }
    }
}

#[component]
fn FailedView(reason: String) -> Element {
    let nav = navigator();
    rsx! {
        div { class: "title-screen",
            div { class: "title-content",
                h1 { class: "title-logo", "Disconnected" }
                p { class: "title-subtitle", "{reason}" }
                button {
                    class: "title-btn title-btn-primary",
                    onclick: move |_| {
                        let _ = nav.push(crate::route::Route::MainMenu {});
                    },
                    div { class: "title-btn-inner", "Back to menu" }
                }
            }
        }
    }
}
