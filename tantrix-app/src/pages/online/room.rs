//! Room content and game context outlive the page-owned connection task.

use dioxus::core::Task;
use dioxus::prelude::*;
use std::{cell::Cell, rc::Rc};

use crate::pages::online::driver::spawn_session;
use crate::pages::online::game::OnlineGameView;
use crate::pages::online::lobby::OnlineLobbyView;
use crate::pages::online::screens::{ConnectingView, FailedView};
use crate::state::nickname;
use crate::state::online_state::{
    ConnectionStatus, FailureReason, OnlinePhase, OnlineState, OnlineStateStoreExt,
};
use crate::state::{room_connection, server_endpoints};

#[component]
pub fn OnlineRoom(code: String) -> Element {
    let server = use_hook(server_endpoints::server_origin);
    let url = use_hook(|| server_endpoints::room_url(&server, &code, false));
    #[cfg(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    ))]
    use_context_provider(|| room_connection::RoomServer(server.clone()));
    let mut attempt = use_signal(|| 0u32);
    let create = use_hook({
        let code = code.clone();
        move || room_connection::take_create(&code)
    });

    let state = use_store(|| OnlineState::new(code.clone()));
    let mut sender = use_signal(|| None);
    let task = use_hook(|| Rc::new(Cell::new(None::<Task>)));
    use_effect(move || {
        let generation = attempt();
        if let Some(previous) = task.take() {
            previous.cancel();
        }
        task.set(Some(spawn_session(
            state,
            sender,
            attempt,
            url.clone(),
            create && generation == 0,
            nickname::load_or_generate(),
        )));
    });
    let on_retry = move |_| {
        // Invalidate old events and commands before the replacement task polls.
        sender.set(None);
        state.connection().set(ConnectionStatus::Connecting);
        attempt += 1;
    };

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
        RoomScreen::Game => rsx! {
            OnlineGameView { state, sender, on_retry }
        },
        RoomScreen::Failed(reason) => rsx! {
            FailedView { reason, on_retry, server }
        },
    }
}

/// Which screen a session shows: one per [`OnlinePhase`], without the
/// lobby's or the game's contents (each screen reads those itself).
#[derive(Debug, Clone, PartialEq)]
enum RoomScreen {
    Connecting,
    Lobby,
    Game,
    Failed(FailureReason),
}

impl RoomScreen {
    fn of(phase: &OnlinePhase) -> Self {
        match phase {
            OnlinePhase::Connecting => Self::Connecting,
            OnlinePhase::Lobby { .. } => Self::Lobby,
            OnlinePhase::Playing { .. } => Self::Game,
            OnlinePhase::Ended { .. } => Self::Game,
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
                RoomScreen::Game,
            ),
            (ended, RoomScreen::Game),
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
