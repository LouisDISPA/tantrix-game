//! Bridge between a `tantrix_net::SessionHandle` and the reactive
//! `Store<OnlineState>` owned by `OnlineRoom`.

use dioxus::prelude::*;
use futures::StreamExt;
use tantrix_net::{open_session, SessionEvent};
use tantrix_proto::RejectCode;

use crate::state::online_state::{
    CommandSender, FailureReason, OnlinePhase, OnlineState, OnlineStateStoreExt,
};
use crate::state::room_connection::room_url;

/// Open a session for the given room and pump events into
/// the `OnlineState` store. The spawned tasks live as long as the component
/// that owns the store is mounted.
pub fn spawn_session(
    store: Store<OnlineState>,
    mut sender: Signal<Option<CommandSender>>,
    room_code: String,
    create: bool,
    nickname: String,
) {
    // This hook also runs while Dioxus server-renders the room page. Only a
    // hydrated browser owns a player connection; SSR must stay on Connecting.
    #[cfg(target_arch = "wasm32")]
    if web_sys::window().is_none() {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if cfg!(feature = "server") {
        return;
    }

    let url = room_url(&room_code, create);
    let (handle, run_fut) = open_session(url, nickname);

    // Publish the command sender so child components can dispatch actions.
    sender.set(Some(CommandSender::new(handle.commands)));

    // Drive the network task.
    spawn(async move {
        run_fut.await;
    });

    // Pipe events into state.
    let mut events = handle.events;
    spawn(async move {
        while let Some(event) = events.next().await {
            apply_event(store, event);
        }
        // The event stream closed without a terminal `Disconnected` — the
        // network task exited early. Park the UI on the Failed screen so
        // the user isn't stuck on the connecting spinner.
        let current = store.phase().read().clone();
        if !matches!(
            current,
            OnlinePhase::Failed { .. } | OnlinePhase::Ended { .. }
        ) {
            store.phase().set(OnlinePhase::Failed {
                reason: FailureReason::Transport("Connection closed unexpectedly.".into()),
            });
        }
    });
}

/// Per-field writes: each event only wakes the specific Store-field
/// subscribers it actually changes. Unrelated UI stays asleep.
fn apply_event(store: Store<OnlineState>, event: SessionEvent) {
    match event {
        SessionEvent::Connected { my_peer_id } => {
            store.my_peer_id().set(Some(my_peer_id));
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
        SessionEvent::StateUpdate {
            snapshot,
            last_move,
        } => {
            store.phase().set(OnlinePhase::Playing {
                snapshot,
                last_move,
            });
        }
        SessionEvent::GameEnded { reason, snapshot } => {
            store.phase().set(OnlinePhase::Ended { reason, snapshot });
        }
        SessionEvent::Error { code, message } => {
            if let Some(code) = code {
                store.last_error().set(Some(code));
            } else {
                log::warn!("online: Error event without code: {message}");
            }
        }
        SessionEvent::ConnectTimeout => {
            log::warn!("online: connecting timed out");
            store.phase().set(OnlinePhase::Failed {
                reason: FailureReason::Timeout,
            });
        }
        SessionEvent::Disconnected { reason } => {
            // The results screen is final: the server keeps the socket open after
            // the game ends, and its later teardown must not replace the
            // scores with a "Disconnected" screen.
            if matches!(&*store.phase().read(), OnlinePhase::Ended { .. }) {
                log::info!("online: ignoring disconnect after the game ended ({reason})");
                return;
            }
            // The server closes after refusing a join. Only those join codes
            // should replace the transport reason; a prior move or color
            // error must not be mistaken for why the socket closed.
            let last_error = store.last_error().read().clone();
            let failure = match last_error {
                Some(code @ (RejectCode::RoomNotFound | RejectCode::RoomFull)) => {
                    store.last_error().set(None);
                    FailureReason::Rejected(code)
                }
                _ => FailureReason::Transport(reason),
            };
            store.phase().set(OnlinePhase::Failed { reason: failure });
        }
    }
}
