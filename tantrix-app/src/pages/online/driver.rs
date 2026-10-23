//! Page-owned connection supervisor. One attempt is fully dropped before
//! opening the next; the room store and displayed game outlive every socket.

use dioxus::core::Task;
use dioxus::prelude::*;
use futures::{FutureExt, StreamExt};
use std::time::Duration;
use tantrix_net::{SessionEvent, SessionHandle};
use tantrix_proto::RejectCode;

use crate::state::online_state::{
    queries, recovery::Recovery, CommandSender, ConnectionStatus, FailureReason, OnlinePhase,
    OnlineState, OnlineStateStoreExt,
};
use crate::util::sleep;

/// This task belongs to OnlineRoom, so navigation cancels the socket,
/// pending backoff and deadlines together. A manual restart cancels it first.
pub fn spawn_session(
    store: Store<OnlineState>,
    mut sender: Signal<Option<CommandSender>>,
    generation: Signal<u32>,
    url: String,
    create: bool,
    nickname: String,
) -> Task {
    spawn(async move {
        #[cfg(not(target_arch = "wasm32"))]
        if cfg!(feature = "server") {
            return;
        }
        let current = *generation.peek();
        // Wall time includes OS sleep where the browser's performance clock pauses.
        let clock = web_time::SystemTime::now();
        let now = || clock.elapsed().unwrap_or_default().as_millis() as u64;
        let mut recovery = Recovery::default();
        let mut recovering = matches!(&*store.phase().peek(), OnlinePhase::Playing { .. });
        if !recovering {
            store.phase().set(OnlinePhase::Connecting);
            store.connection().set(ConnectionStatus::Connecting);
        }
        loop {
            sender.set(None);
            let deadline = if recovering {
                let Some(retry) = recovery.disconnected(now()) else {
                    store.connection().set(ConnectionStatus::Stopped);
                    return;
                };
                store.connection().set(ConnectionStatus::Reconnecting {
                    attempt: retry.number,
                });
                sleep(Duration::from_millis(retry.start.saturating_sub(now()))).await;
                if *generation.peek() != current {
                    return;
                }
                // After background suspension, never open an expired attempt.
                if now() >= retry.deadline {
                    store.connection().set(ConnectionStatus::Stopped);
                    return;
                }
                retry.deadline
            } else {
                now() + 20_000
            };
            let attempt_url = if create && !recovering {
                format!("{url}?create=1")
            } else {
                url.clone()
            };
            if let Some(since) = run_attempt(
                store,
                sender,
                generation,
                attempt_url,
                nickname.clone(),
                deadline,
                clock,
            )
            .await
            {
                recovery.synchronized(since);
            }
            sender.set(None);
            if *generation.peek() != current
                || matches!(
                    *store.connection().peek(),
                    ConnectionStatus::OpenedElsewhere
                )
            {
                return;
            }
            if !matches!(&*store.phase().peek(), OnlinePhase::Playing { .. }) {
                return;
            }
            recovering = true;
        }
    })
}

async fn run_attempt(
    store: Store<OnlineState>,
    mut sender: Signal<Option<CommandSender>>,
    generation: Signal<u32>,
    url: String,
    nickname: String,
    deadline: u64,
    clock: web_time::SystemTime,
) -> Option<u64> {
    let current = *generation.peek();
    let previous_seat = queries::my_color_of(store);
    let recovering = matches!(&*store.phase().peek(), OnlinePhase::Playing { .. });
    #[cfg(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    ))]
    let (handle, run) = {
        let cookie = match crate::state::server_settings::session_cookie(&url) {
            Ok(cookie) => cookie,
            Err(reason) => {
                apply_event(store, SessionEvent::Disconnected { reason });
                return None;
            }
        };
        tantrix_net::open_native_session(url, nickname, cookie)
    };
    #[cfg(not(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    )))]
    let (handle, run) = tantrix_net::open_session(url, nickname);
    let SessionHandle {
        commands,
        mut events,
    } = handle;
    let run = run.fuse();
    futures::pin_mut!(run);
    let mut synchronized = None;
    let mut pending_identity = None;
    loop {
        let now = clock.elapsed().unwrap_or_default().as_millis() as u64;
        if *generation.peek() != current {
            return synchronized;
        }
        if synchronized.is_none() && now >= deadline {
            apply_event(store, SessionEvent::ConnectTimeout);
            return synchronized;
        }
        let timeout = async move {
            if synchronized.is_some() {
                futures::future::pending::<()>().await;
            }
            sleep(Duration::from_millis(deadline.saturating_sub(now))).await;
        }
        .fuse();
        futures::pin_mut!(timeout);
        futures::select_biased! {
            _ = timeout => {
                if *generation.peek() != current { return synchronized; }
                apply_event(store, SessionEvent::ConnectTimeout);
                return synchronized;
            },
            event = events.next().fuse() => {
                if *generation.peek() != current { return synchronized; }
                let Some(event) = event else { events_closed(store); return synchronized; };
                // Keep the cached seat identity until its matching snapshot arrives.
                // A welcomed attempt can still time out; it must not erase which
                // seat we held before recovery on the next attempt.
                if recovering && synchronized.is_none() {
                    match &event {
                        SessionEvent::Connected { my_peer_id } => {
                            pending_identity = Some(my_peer_id.clone());
                            continue;
                        }
                        SessionEvent::GameStarted { .. } | SessionEvent::StateUpdate { .. } | SessionEvent::GameEnded { .. } => {
                            if let Some(peer_id) = pending_identity.take() {
                                store.my_peer_id().set(Some(peer_id));
                            }
                        }
                        _ => {},
                    }
                }
                let terminal = matches!(event, SessionEvent::Disconnected { .. } | SessionEvent::OpenedElsewhere | SessionEvent::ConnectTimeout);
                apply_event(store, event);
                if terminal { return synchronized; }
                if store.connection().peek().is_connected() && synchronized.is_none() {
                    synchronized = Some(clock.elapsed().unwrap_or_default().as_millis() as u64);
                    if recovering {
                        store.seat_changed().set(previous_seat.is_some() && queries::my_color_of(store).is_none());
                    }
                    sender.set(Some(CommandSender::new(commands.clone())));
                }
            },
            _ = run => {},
        }
    }
}

/// The event stream closed. Without a terminal `Disconnected` first, the
/// network task exited early. Recover a cached game or show the initial
/// failure screen; a finished game keeps its results.
fn events_closed(store: Store<OnlineState>) {
    if matches!(
        &*store.phase().peek(),
        OnlinePhase::Failed { .. } | OnlinePhase::Ended { .. }
    ) {
        return;
    }
    apply_event(
        store,
        SessionEvent::Disconnected {
            reason: "Connection closed unexpectedly.".into(),
        },
    );
}

/// Per-field writes: each event only wakes the specific Store-field
/// subscribers it actually changes. Unrelated UI stays asleep.
fn apply_event(store: Store<OnlineState>, event: SessionEvent) {
    match event {
        SessionEvent::Connected { my_peer_id } => {
            store.my_peer_id().set(Some(my_peer_id));
        }
        SessionEvent::Lobby(lobby) => {
            // Welcome includes lobby information even for a running game.
            // It is identity only until the fresh game snapshot arrives.
            if !matches!(
                &*store.phase().peek(),
                OnlinePhase::Playing { .. } | OnlinePhase::Ended { .. }
            ) {
                store.phase().set(OnlinePhase::Lobby { lobby });
                store.connection().set(ConnectionStatus::Connected);
                store.last_error().set(None);
            }
        }
        SessionEvent::GameStarted { snapshot } => {
            store.phase().set(OnlinePhase::Playing {
                snapshot,
                last_move: None,
            });
            store.connection().set(ConnectionStatus::Connected);
            store.last_error().set(None);
        }
        SessionEvent::StateUpdate {
            snapshot,
            last_move,
        } => {
            store.connection().set(ConnectionStatus::Connected);
            store.phase().set(OnlinePhase::Playing {
                snapshot,
                last_move,
            });
            store.last_error().set(None);
        }
        SessionEvent::GameEnded { reason, snapshot } => {
            store.connection().set(ConnectionStatus::Connected);
            store.phase().set(OnlinePhase::Ended { reason, snapshot });
            store.last_error().set(None);
        }
        SessionEvent::Error { code, message } => {
            if let Some(code) = code {
                store.last_error().set(Some(code));
            } else {
                log::warn!("online: Error event without code: {message}");
            }
        }
        SessionEvent::ConnectTimeout => {
            store.connection().set(ConnectionStatus::Stopped);
            if !matches!(
                &*store.phase().peek(),
                OnlinePhase::Playing { .. } | OnlinePhase::Ended { .. }
            ) {
                store.phase().set(OnlinePhase::Failed {
                    reason: FailureReason::Timeout,
                });
            }
        }
        SessionEvent::OpenedElsewhere => {
            store.connection().set(ConnectionStatus::OpenedElsewhere);
            store.last_error().set(None);
            if !matches!(
                &*store.phase().peek(),
                OnlinePhase::Playing { .. } | OnlinePhase::Ended { .. }
            ) {
                store.phase().set(OnlinePhase::Failed {
                    reason: FailureReason::Transport(tantrix_proto::OPENED_ELSEWHERE.into()),
                });
            }
        }
        SessionEvent::Disconnected { reason } => {
            store.connection().set(ConnectionStatus::Stopped);
            // The results screen is final: the server keeps the socket open after
            // the game ends, and its later teardown must not replace the
            // scores with a "Disconnected" screen.
            if matches!(&*store.phase().read(), OnlinePhase::Ended { .. }) {
                log::info!("online: ignoring disconnect after the game ended ({reason})");
                return;
            }
            if matches!(&*store.phase().peek(), OnlinePhase::Playing { .. }) {
                store.last_error().set(None);
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

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;
    use tantrix_game::{Board, Color, EnumMap, Move, Position, Rotation, Tile, TurnStep};
    use tantrix_net::SessionEvent;
    use tantrix_proto::{EndReason, GameSnapshot, LobbyState, PeerId, RejectCode, Seat};

    use super::{apply_event, events_closed};
    use crate::state::online_state::{ConnectionStatus, FailureReason, OnlinePhase, OnlineState};

    /// What the session closes a refused socket with (`tantrix-net`).
    const REJECTED: &str = "The room rejected this connection.";
    /// The room's close reason for the older socket of a player who opened
    /// the room again (`tantrix_room::engine::OPENED_ELSEWHERE`).
    const OPENED_ELSEWHERE: &str = "This room was opened in another tab.";

    /// A room session's state, kept in a `Store` as `OnlineRoomSession`
    /// keeps it, once the session's `events` are applied to it in order —
    /// and then the end of the event stream, if `close`.
    fn session(events: impl IntoIterator<Item = SessionEvent>, close: bool) -> OnlineState {
        let mut dom = VirtualDom::new(|| {
            let state = use_store(|| OnlineState::new("ABCDEF".to_string()));
            use_context_provider(|| state);
            rsx! {}
        });
        dom.rebuild_in_place();
        dom.in_scope(ScopeId::APP, || {
            let store = consume_context::<Store<OnlineState>>();
            for event in events {
                apply_event(store, event);
            }
            if close {
                events_closed(store);
            }
            // No reactive context here: reading subscribes nothing.
            store.cloned()
        })
    }

    fn after(events: impl IntoIterator<Item = SessionEvent>) -> OnlineState {
        session(events, false)
    }

    fn after_closing(events: impl IntoIterator<Item = SessionEvent>) -> OnlinePhase {
        session(events, true).phase
    }

    fn me() -> PeerId {
        PeerId::new("me")
    }

    fn welcome() -> SessionEvent {
        SessionEvent::Connected { my_peer_id: me() }
    }

    /// A lobby where `waiting` wait for a seat.
    fn lobby(waiting: &[&str]) -> LobbyState {
        LobbyState {
            seats: EnumMap::default(),
            unseated: waiting
                .iter()
                .map(|&name| Seat {
                    peer_id: PeerId::new(name),
                    nickname: name.to_string(),
                })
                .collect(),
            participants: Vec::new(),
        }
    }

    /// A game snapshot, told apart by its bag count.
    fn snapshot(bag_count: usize) -> GameSnapshot {
        GameSnapshot {
            board: Board::default(),
            hands: EnumMap::default(),
            bag_count,
            current_player: Color::Red,
            turn_step: TurnStep::BeforeFreeMove,
            playing_colors: vec![Color::Red, Color::Blue],
            seats: EnumMap::default(),
            participants: Vec::new(),
        }
    }

    fn rejected(code: RejectCode) -> SessionEvent {
        SessionEvent::Error {
            code: Some(code),
            message: "refused".to_string(),
        }
    }

    fn closed(reason: &str) -> SessionEvent {
        SessionEvent::disconnected(reason.to_string())
    }

    fn failed(reason: FailureReason) -> OnlinePhase {
        OnlinePhase::Failed { reason }
    }

    fn ended() -> SessionEvent {
        SessionEvent::GameEnded {
            reason: EndReason::Completed {
                winners: vec![Color::Blue],
            },
            snapshot: snapshot(0),
        }
    }

    #[test]
    fn recovery_welcome_keeps_the_cached_game_until_a_fresh_snapshot() {
        let initial = SessionEvent::GameStarted {
            snapshot: snapshot(40),
        };
        let recovering = [
            initial.clone(),
            closed("lost"),
            welcome(),
            SessionEvent::Lobby(lobby(&[])),
        ];
        let cached = after(recovering.clone());
        assert_eq!(cached.phase, after([initial]).phase);
        assert_eq!(cached.connection, ConnectionStatus::Stopped);
        let synchronized = after(recovering.into_iter().chain([SessionEvent::GameStarted {
            snapshot: snapshot(38),
        }]));
        assert_eq!(synchronized.connection, ConnectionStatus::Connected);
        assert!(
            matches!(synchronized.phase, OnlinePhase::Playing { snapshot: game, .. } if game.bag_count == 38)
        );
    }

    #[test]
    fn a_welcome_names_this_peer_and_opens_the_lobby() {
        // The room's `Welcome` arrives as these two events.
        let connected = after([welcome()]);
        assert_eq!(connected.my_peer_id, Some(me()));
        assert_eq!(connected.phase, OnlinePhase::Connecting);

        let state = after([welcome(), SessionEvent::Lobby(lobby(&["Ann"]))]);
        assert_eq!(
            state,
            OnlineState {
                room_code: "ABCDEF".to_string(),
                my_peer_id: Some(me()),
                phase: OnlinePhase::Lobby {
                    lobby: lobby(&["Ann"])
                },
                last_error: None,
                connection: ConnectionStatus::Connected,
                seat_changed: false,
            }
        );
    }

    #[test]
    fn a_rejected_welcome_fails_with_the_rooms_reason() {
        for code in [RejectCode::RoomFull, RejectCode::RoomNotFound] {
            let state = after([rejected(code.clone()), closed(REJECTED)]);
            assert_eq!(state.phase, failed(FailureReason::Rejected(code.clone())));
            // The failure screen says it; the error is not kept to show twice.
            assert_eq!(state.last_error, None);
            assert!(!FailureReason::Rejected(code).is_retryable());
        }
    }

    #[test]
    fn a_room_opened_in_another_tab_says_so() {
        let lobby_then_closed = after([
            welcome(),
            SessionEvent::Lobby(lobby(&[])),
            closed(OPENED_ELSEWHERE),
        ]);
        let in_game_then_closed = after([
            welcome(),
            SessionEvent::GameStarted {
                snapshot: snapshot(40),
            },
            closed(OPENED_ELSEWHERE),
        ]);
        assert_eq!(
            lobby_then_closed.connection,
            ConnectionStatus::OpenedElsewhere
        );
        assert_eq!(
            in_game_then_closed.connection,
            ConnectionStatus::OpenedElsewhere
        );
        assert!(matches!(
            in_game_then_closed.phase,
            OnlinePhase::Playing { .. }
        ));
    }

    #[test]
    fn an_earlier_rejection_is_not_why_the_socket_closed() {
        // A refused color claim, then the connection drops: the drop is the
        // failure, not the claim.
        let state = after([
            welcome(),
            SessionEvent::Lobby(lobby(&[])),
            rejected(RejectCode::ColorTaken),
            closed("Connection closed."),
        ]);
        assert_eq!(
            state.phase,
            failed(FailureReason::Transport("Connection closed.".to_string()))
        );
    }

    #[test]
    fn a_rejected_intent_shows_until_the_lobby_or_the_game_moves_on() {
        let refused = [
            welcome(),
            SessionEvent::Lobby(lobby(&["Ann"])),
            rejected(RejectCode::ColorTaken),
        ];
        let state = after(refused.clone());
        assert_eq!(state.last_error, Some(RejectCode::ColorTaken));
        // The error leaves the lobby as it was.
        assert_eq!(
            state.phase,
            OnlinePhase::Lobby {
                lobby: lobby(&["Ann"])
            }
        );

        let updated = after(
            refused
                .clone()
                .into_iter()
                .chain([SessionEvent::Lobby(lobby(&[]))]),
        );
        assert_eq!(updated.last_error, None);
        let started = after(refused.into_iter().chain([SessionEvent::GameStarted {
            snapshot: snapshot(40),
        }]));
        assert_eq!(started.last_error, None);
    }

    #[test]
    fn game_rejections_preserve_the_position_and_replace_the_previous_error() {
        let started = SessionEvent::GameStarted {
            snapshot: snapshot(40),
        };
        let before = after([started.clone()]);
        let refused = after([started.clone(), rejected(RejectCode::NotYourTurn)]);
        assert_eq!(refused.phase, before.phase);
        assert_eq!(refused.last_error, Some(RejectCode::NotYourTurn));

        let replaced = after([
            started,
            rejected(RejectCode::NotYourTurn),
            rejected(RejectCode::InvalidMove),
        ]);
        assert_eq!(replaced.phase, before.phase);
        assert_eq!(replaced.last_error, Some(RejectCode::InvalidMove));
    }

    #[test]
    fn moves_seat_updates_and_game_end_clear_rejections() {
        let m = Move {
            tile: Tile::BYYBRR,
            position: Position::CENTER,
            rotation: Rotation::R60,
            forced: false,
        };
        let mut moved = snapshot(39);
        moved.board.insert_move(m).unwrap();
        let mut reseated = snapshot(40);
        reseated.seats[Color::Red] = Some(Seat {
            peer_id: me(),
            nickname: "Ann".to_string(),
        });
        for update in [
            SessionEvent::StateUpdate {
                snapshot: moved,
                last_move: Some(m),
            },
            SessionEvent::StateUpdate {
                snapshot: reseated,
                last_move: None,
            },
            ended(),
        ] {
            let expected = after([update.clone()]);
            let state = after([
                SessionEvent::GameStarted {
                    snapshot: snapshot(40),
                },
                rejected(RejectCode::InvalidMove),
                update,
            ]);
            assert_eq!(state.last_error, None);
            assert_eq!(state.phase, expected.phase);
        }
    }

    #[test]
    fn an_error_without_a_code_changes_nothing() {
        let before = [welcome(), SessionEvent::Lobby(lobby(&["Ann"]))];
        let state = after(before.clone().into_iter().chain([SessionEvent::Error {
            code: None,
            message: "unknown".to_string(),
        }]));
        assert_eq!(state, after(before));
    }

    #[test]
    fn the_game_starts_and_each_snapshot_replaces_the_last() {
        let started = after([
            welcome(),
            SessionEvent::GameStarted {
                snapshot: snapshot(40),
            },
        ]);
        assert_eq!(
            started.phase,
            OnlinePhase::Playing {
                snapshot: snapshot(40),
                last_move: None,
            }
        );

        let m = Move {
            tile: Tile::BYYBRR,
            position: Position::CENTER,
            rotation: Rotation::R60,
            forced: false,
        };
        let moved = after([
            welcome(),
            SessionEvent::GameStarted {
                snapshot: snapshot(40),
            },
            SessionEvent::StateUpdate {
                snapshot: snapshot(39),
                last_move: Some(m),
            },
        ]);
        assert_eq!(
            moved.phase,
            OnlinePhase::Playing {
                snapshot: snapshot(39),
                last_move: Some(m),
            }
        );
        assert_eq!(moved.my_peer_id, Some(me()));
    }

    #[test]
    fn the_results_outlast_the_socket() {
        let game_then_end = [
            welcome(),
            SessionEvent::GameStarted {
                snapshot: snapshot(40),
            },
            ended(),
        ];
        let results = after(game_then_end.clone()).phase;
        assert!(matches!(results, OnlinePhase::Ended { .. }));
        // The server closes the socket after the game: the scores stay.
        let closed_after = after(game_then_end.clone().into_iter().chain([closed("Bye.")]));
        assert_eq!(closed_after.phase, results);
        assert_eq!(after_closing(game_then_end), results);
    }

    #[test]
    fn connecting_too_long_fails_and_can_be_retried() {
        let phase = after([SessionEvent::ConnectTimeout]).phase;
        assert_eq!(phase, failed(FailureReason::Timeout));
        assert!(FailureReason::Timeout.is_retryable());
    }

    #[test]
    fn a_stream_closing_early_does_not_leave_the_spinner_up() {
        let unexpected = failed(FailureReason::Transport(
            "Connection closed unexpectedly.".to_string(),
        ));
        assert_eq!(after_closing([]), unexpected);
        assert_eq!(
            after_closing([welcome(), SessionEvent::Lobby(lobby(&[]))]),
            unexpected
        );
        // A failure already shown keeps its reason.
        assert_eq!(
            after_closing([SessionEvent::ConnectTimeout]),
            failed(FailureReason::Timeout)
        );
    }
}
