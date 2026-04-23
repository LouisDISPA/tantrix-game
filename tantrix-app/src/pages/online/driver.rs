//! Bridge between a `tantrix_net::SessionHandle` and the reactive
//! `Store<OnlineState>` owned by `OnlineRoom`.

use dioxus::prelude::*;
use futures::StreamExt;
use tantrix_net::{
    SessionEvent, SessionHandle, SessionRole, open_client_session, open_host_session,
};
use tantrix_proto::RejectCode;

use crate::state::online_state::{
    CommandSender, FailureReason, OnlinePhase, OnlineState, OnlineStateStoreExt,
};
use crate::state::session_token;
use crate::state::signaling::signaling_url;

/// Open a session (host or client) for the given room and pump events into
/// the `OnlineState` store. The spawned tasks live as long as the component
/// that owns the store is mounted.
pub fn spawn_session(
    store: Store<OnlineState>,
    mut sender: Signal<Option<CommandSender>>,
    room_code: String,
    role: SessionRole,
    nickname: String,
) {
    let url = signaling_url(&room_code);

    let (handle, run_fut): (SessionHandle, _) = match role {
        SessionRole::Host => {
            let (h, f) = open_host_session(url, nickname);
            (h, futures::future::Either::Left(f))
        }
        SessionRole::Client => {
            let token = session_token::load_or_create(&room_code);
            let (h, f) = open_client_session(url, nickname, token);
            (h, futures::future::Either::Right(f))
        }
    };

    // Publish the command sender so child components can dispatch actions.
    sender.set(Some(CommandSender::new(handle.commands)));

    // Drive the network task.
    spawn(async move {
        run_fut.await;
    });

    // Pipe events into state.
    let mut events = handle.events;
    let room_for_events = room_code.clone();
    spawn(async move {
        while let Some(event) = events.next().await {
            apply_event(store, event, &room_for_events);
        }
        // The event stream closed without a terminal `Disconnected` — the
        // network task exited early. Park the UI on the Failed screen so
        // the user isn't stuck on the connecting spinner.
        let current = store.phase().read().clone();
        if !matches!(current, OnlinePhase::Failed { .. } | OnlinePhase::Ended { .. }) {
            store.phase().set(OnlinePhase::Failed {
                reason: FailureReason::Transport("Connection closed unexpectedly.".into()),
            });
        }
    });
}

/// Per-field writes: each event only wakes the specific Store-field
/// subscribers it actually changes. Unrelated UI stays asleep.
fn apply_event(store: Store<OnlineState>, event: SessionEvent, room_code: &str) {
    match event {
        SessionEvent::Connected {
            role: _,
            my_peer_id,
            host_peer_id,
        } => {
            store.my_peer_id().set(Some(my_peer_id));
            store.host_peer_id().set(Some(host_peer_id));
        }
        SessionEvent::Lobby(lobby) => {
            store.phase().set(OnlinePhase::Lobby { lobby });
            store.last_error().set(None);
        }
        SessionEvent::GameStarted { snapshot } => {
            store.phase().set(OnlinePhase::Playing {
                snapshot,
                last_move: None,
            });
            store.last_error().set(None);
        }
        SessionEvent::StateUpdate { snapshot, last_move } => {
            store.phase().set(OnlinePhase::Playing { snapshot, last_move });
        }
        SessionEvent::GameEnded { reason: _, snapshot } => {
            store.phase().set(OnlinePhase::Ended { snapshot });
        }
        SessionEvent::Error { code, message } => {
            if matches!(code, Some(RejectCode::Kicked)) {
                // Drop our stored token so a fresh rejoin mints a new one
                // instead of immediately re-tripping the kick-seen record.
                session_token::clear(room_code);
            }
            if let Some(code) = code {
                store.last_error().set(Some(code));
            } else {
                log::warn!("online: Error event without code: {message}");
            }
        }
        SessionEvent::Disconnected { reason } => {
            // Prefer a structured host-reject over the generic transport
            // reason. Consume last_error (take) so a later reconnect isn't
            // seeded with a stale value.
            let last_error = store.last_error().read().clone();
            let failure = match last_error {
                Some(code) => {
                    store.last_error().set(None);
                    FailureReason::Rejected(code)
                }
                None => FailureReason::Transport(reason),
            };
            store.phase().set(OnlinePhase::Failed { reason: failure });
        }
    }
}
