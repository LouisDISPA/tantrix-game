//! `/online/:code` — root component that owns the session.
//!
//! Creates the `Store<OnlineState>` and the command-sender signal locally
//! and passes them down as props to each child screen. Desktop rooms also
//! provide the captured server origin to the invitation UI.
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
use crate::state::online_state::{FailureReason, OnlinePhase, OnlineState, OnlineStateStoreExt};
use crate::state::room_connection;

#[component]
pub fn OnlineRoom(code: String) -> Element {
    let url = use_hook(|| room_connection::room_url(&code, false));
    #[cfg(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    ))]
    use_context_provider(|| {
        room_connection::RoomServer(
            url.split("/room/")
                .next()
                .unwrap_or(&url)
                .replacen("ws", "http", 1),
        )
    });
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
                url: if create && attempt_no == 0 { format!("{url}?create=1") } else { url.clone() },
                on_retry: move |_| {
                    log::info!("online: retrying (attempt {})", attempt() + 2);
                    attempt += 1;
                },
            }
        }
    }
}

#[component]
fn OnlineRoomSession(code: String, url: String, on_retry: EventHandler<()>) -> Element {
    let state = use_store(|| OnlineState::new(code.clone()));
    let sender = use_signal(|| None);

    // Once per session: the stored nickname (or a fresh one) is read when the
    // session opens, not on every render.
    use_hook(|| {
        spawn_session(state, sender, url.clone(), nickname::load_or_generate());
    });

    // The screen for the room's phase. A memo, so a lobby update or a game
    // snapshot (which keep the screen) does not re-render this component or
    // clone the phase.
    let screen = use_memo(move || RoomScreen::of(&state.phase().read()));
    match screen() {
        RoomScreen::Connecting => rsx! {
            ConnectingView { state, on_retry }
        },
        RoomScreen::Lobby => rsx! {
            OnlineLobbyView { state, sender }
        },
        RoomScreen::Playing => rsx! {
            OnlineGameView { state, sender }
        },
        RoomScreen::Ended => rsx! {
            OnlineGameView { state, sender }
        },
        RoomScreen::Failed(reason) => rsx! {
            FailedView { reason, on_retry, server: url }
        },
    }
}

/// Which screen a session shows: one per [`OnlinePhase`], without the
/// lobby's or the game's contents (each screen reads those itself).
#[derive(Debug, Clone, PartialEq)]
enum RoomScreen {
    Connecting,
    Lobby,
    Playing,
    Ended,
    Failed(FailureReason),
}

impl RoomScreen {
    fn of(phase: &OnlinePhase) -> Self {
        match phase {
            OnlinePhase::Connecting => Self::Connecting,
            OnlinePhase::Lobby { .. } => Self::Lobby,
            OnlinePhase::Playing { .. } => Self::Playing,
            OnlinePhase::Ended { .. } => Self::Ended,
            OnlinePhase::Failed { reason } => Self::Failed(reason.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use tantrix_game::Color::{Blue, Red};
    use tantrix_proto::EndReason;

    use super::RoomScreen;
    use crate::state::online_state::test_rooms::{lobby, snapshot};
    use crate::state::online_state::{FailureReason, OnlinePhase};

    #[test]
    fn each_phase_has_its_screen() {
        let game = snapshot(&[(Red, "Ann", true), (Blue, "Bob", true)], &[]);
        let ended = OnlinePhase::Ended {
            reason: EndReason::Completed { winners: vec![Red] },
            snapshot: game.clone(),
        };
        let failed = OnlinePhase::Failed {
            reason: FailureReason::Timeout,
        };
        let cases = [
            (OnlinePhase::Connecting, RoomScreen::Connecting),
            (
                OnlinePhase::Lobby {
                    lobby: lobby(&[], &["Ann"]),
                },
                RoomScreen::Lobby,
            ),
            (
                OnlinePhase::Playing {
                    snapshot: game,
                    last_move: None,
                },
                RoomScreen::Playing,
            ),
            (ended, RoomScreen::Ended),
            (failed, RoomScreen::Failed(FailureReason::Timeout)),
        ];
        for (phase, screen) in cases {
            assert_eq!(RoomScreen::of(&phase), screen);
        }
    }

    #[test]
    fn updates_within_a_phase_keep_the_screen() {
        // What keeps the session from re-rendering on every lobby change
        // or game snapshot: its screen memo holds the same value.
        let before = OnlinePhase::Lobby {
            lobby: lobby(&[], &["Ann"]),
        };
        let after = OnlinePhase::Lobby {
            lobby: lobby(&[(Red, "Ann", true)], &["Bob"]),
        };
        assert_eq!(RoomScreen::of(&before), RoomScreen::of(&after));

        let mut moved = snapshot(&[(Red, "Ann", true), (Blue, "Bob", true)], &[]);
        let first = OnlinePhase::Playing {
            snapshot: moved.clone(),
            last_move: None,
        };
        moved.bag_count -= 1;
        moved.current_player = Blue;
        let next = OnlinePhase::Playing {
            snapshot: moved,
            last_move: None,
        };
        assert_eq!(RoomScreen::of(&first), RoomScreen::of(&next));

        // A failure's reason is part of its screen.
        let timeout = OnlinePhase::Failed {
            reason: FailureReason::Timeout,
        };
        let dropped = OnlinePhase::Failed {
            reason: FailureReason::Transport("Connection closed.".to_string()),
        };
        assert_ne!(RoomScreen::of(&timeout), RoomScreen::of(&dropped));
    }
}
