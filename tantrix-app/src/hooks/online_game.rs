//! Bridge a snapshot stream from `Store<OnlineState>` into the `GameCtx`
//! of the shared game screens.
//!
//! The context comes from [`use_game_ctx`] with an online [`GameDriver`]:
//! moves go out through the session's command sender, and the seat this
//! device plays and the players' nicknames are memos over the online state.
//! One effect fans each inbound snapshot out into `game_state` / `ui_state`.

use std::cell::Cell;
use std::rc::Rc;

use dioxus::prelude::*;
use tantrix_game::{Color, EnumMap};
use tantrix_proto::{GameSnapshot, Seat};

use crate::hooks::camera::BoardCamera;
use crate::hooks::use_game_ctx;
use crate::state::game_driver::{GameDriver, RoomLink};
use crate::state::game_state::{GameCtx, GameState};
use crate::state::online_state::{
    queries, CommandSender, OnlinePhase, OnlineState, OnlineStateStoreExt,
};
use crate::state::ui_state::{
    set_if_changed, TurnPhase, UiState, UiStateStoreExt, UiStateStoreImplExt,
};

/// Build the `GameCtx` for an online game, dealt from the room's latest
/// snapshot and kept live via reactive effects on the surrounding online
/// store + sender. Call it once the room has sent a snapshot (in the
/// Playing or Ended phase).
pub fn use_online_game_ctx(
    state: Store<OnlineState>,
    sender: Signal<Option<CommandSender>>,
) -> GameCtx {
    let my_color = use_memo(move || queries::my_color_of(state));
    let names = use_memo(move || player_names_of(state));
    let connected = use_memo(move || state.connection().read().is_connected());
    let driver = GameDriver::Online(RoomLink::new(sender, my_color, names, connected.into()));
    // The first snapshot is read once, without subscribing: the effect below
    // brings in every later one.
    let ctx = use_game_ctx(driver, move || {
        let phase = state.phase();
        let phase = phase.peek();
        let snapshot = snapshot_in(&phase).expect("the online game starts from a snapshot");
        GameState::from_snapshot(snapshot)
    });

    let mut ui = ctx.ui_state;
    use_effect(move || {
        if !state.connection().read().is_connected() {
            ui.reset_selection();
        }
    });
    fan_out_snapshots(state, ctx.game_state, ctx.ui_state);
    ctx
}

/// Whether two games are at the same point: the same tiles down, the same
/// player to move at the same step, the same bag.
fn same_position(a: &GameState, b: &GameState) -> bool {
    a.board().len() == b.board().len()
        && a.current_player() == b.current_player()
        && a.turn_step() == b.turn_step()
        && a.bag_count() == b.bag_count()
}

/// The Playing/Ended snapshot in `phase`, if any.
fn snapshot_in(phase: &OnlinePhase) -> Option<&GameSnapshot> {
    match phase {
        OnlinePhase::Playing { snapshot, .. } | OnlinePhase::Ended { snapshot, .. } => {
            Some(snapshot)
        }
        _ => None,
    }
}

fn fan_out_snapshots(
    state: Store<OnlineState>,
    mut game_state: Signal<GameState>,
    mut ui_state: Store<UiState>,
) {
    // The seat this device held when the last snapshot came in.
    let last_seat = use_hook(|| Rc::new(Cell::new(None::<Option<Color>>)));
    use_effect(move || {
        let phase = state.phase();
        let phase = phase.read();

        let (snapshot, last_move, terminal) = match &*phase {
            OnlinePhase::Playing {
                snapshot,
                last_move,
            } => (snapshot, last_move.as_ref(), false),
            OnlinePhase::Ended { snapshot, .. } => (snapshot, None, true),
            _ => return,
        };

        let game = GameState::from_snapshot(snapshot);
        let placed: Vec<_> = game.board().iter().map(|m| m.position).collect();
        // A snapshot that changes nothing about the game (someone joined or
        // left the room) must not close the card the player is choosing from.
        let seat = queries::my_color_of(state);
        let unchanged =
            same_position(&game_state.peek(), &game) && last_seat.replace(Some(seat)) == Some(seat);
        *game_state.write() = game;

        // Field by field (and never reading `ui_state`, which would make this
        // effect rerun on its own writes): a snapshot leaves the camera, the
        // animation hints and the scoring overlay alone.
        if terminal {
            ui_state.clear_selection();
            set_if_changed(ui_state.phase(), TurnPhase::GameOver);
        } else {
            if !unchanged {
                ui_state.reset_selection();
            }
            if let Some(m) = last_move {
                set_if_changed(ui_state.last_placed_position(), Some(m.position));
                // The camera follows a move that arrives, as it does one
                // played here.
                BoardCamera::new(ui_state).frame_board(&placed);
            }
        }
    });
}

/// Each color's nickname, from the seats in the game's latest snapshot.
fn player_names_of(state: Store<OnlineState>) -> EnumMap<Color, Option<String>> {
    let phase = state.phase();
    let phase = phase.read();
    match &*phase {
        OnlinePhase::Playing { snapshot, .. } | OnlinePhase::Ended { snapshot, .. } => {
            names_from_seats(&snapshot.seats)
        }
        _ => EnumMap::default(),
    }
}

fn names_from_seats(seats: &EnumMap<Color, Option<Seat>>) -> EnumMap<Color, Option<String>> {
    EnumMap::from_fn(|c| seats[c].as_ref().map(|s| s.nickname.clone()))
}

#[cfg(test)]
mod tests {
    use tantrix_game::Color::{self, Blue, Green, Red, Yellow};
    use tantrix_proto::EndReason;

    use super::{names_from_seats, same_position, snapshot_in};
    use crate::state::online_state::test_rooms::{lobby, snapshot};
    use crate::state::online_state::{FailureReason, OnlinePhase};
    use crate::state::test_games::seeded;

    #[test]
    fn a_snapshot_that_moves_nothing_is_the_same_position() {
        let before = seeded(3);
        assert!(same_position(&before, &seeded(3)));

        let mut after = before.clone();
        let m = after.legal_moves()[0];
        after.play_move(m.placement()).unwrap();
        assert!(!same_position(&before, &after));
    }

    #[test]
    fn the_game_comes_from_a_running_or_finished_room() {
        let game = snapshot(&[(Red, "Ann", true), (Blue, "Bob", true)], &[]);
        let playing = OnlinePhase::Playing {
            snapshot: game.clone(),
            last_move: None,
        };
        let ended = OnlinePhase::Ended {
            reason: EndReason::EndedEarly {
                by: Blue,
                winners: vec![Red, Blue],
            },
            snapshot: game.clone(),
        };
        assert_eq!(snapshot_in(&playing), Some(&game));
        assert_eq!(snapshot_in(&ended), Some(&game));

        let lobby = OnlinePhase::Lobby {
            lobby: lobby(&[(Red, "Ann", true)], &[]),
        };
        let failed = OnlinePhase::Failed {
            reason: FailureReason::Timeout,
        };
        for phase in [OnlinePhase::Connecting, lobby, failed] {
            assert_eq!(snapshot_in(&phase), None, "{phase:?}");
        }
    }

    #[test]
    fn each_seat_is_named_after_its_player() {
        // Bob is offline but keeps his seat, and his name with it.
        let game = snapshot(&[(Blue, "Bob", false), (Yellow, "Cy", true)], &["Dee"]);
        let names = names_from_seats(&game.seats);
        let name = |color: Color| names[color].as_deref();
        assert_eq!(name(Blue), Some("Bob"));
        assert_eq!(name(Yellow), Some("Cy"));
        // Nobody sits at the others; watchers have no color.
        assert_eq!(name(Red), None);
        assert_eq!(name(Green), None);
    }

    #[test]
    fn losing_connection_cancels_selection_and_drag_and_blocks_dispatch_without_moving_camera() {
        use crate::state::online_state::{
            CommandSender, ConnectionStatus, OnlineState, OnlineStateStoreExt,
        };
        use crate::state::ui_state::{TurnPhase, UiStateStoreExt};
        use crate::state::{
            actions,
            game_state::GameCtx,
            gesture::Pointer,
            tile_drag::{move_tile, press_tile},
        };
        use crate::svg::geometry::Rect;
        use crate::util::test_page::settle;
        use dioxus::prelude::*;
        use futures_channel::mpsc::unbounded;
        use tantrix_game::{EnumMap, Position, Rotation};
        use tantrix_proto::{GameSnapshot, PeerId, Seat};

        let mut dom = VirtualDom::new(|| {
            let game = seeded(3);
            let current = game.current_player();
            let state = use_store(|| {
                let mut state = OnlineState::new("ABCDEF".into());
                state.my_peer_id = Some(PeerId::new("me"));
                state.connection = ConnectionStatus::Connected;
                state.phase = OnlinePhase::Playing {
                    snapshot: GameSnapshot {
                        board: game.board().clone(),
                        hands: EnumMap::from_fn(|color| game.hand(color).clone()),
                        bag_count: game.bag_count(),
                        current_player: current,
                        turn_step: game.turn_step(),
                        playing_colors: game.playing_colors().to_vec(),
                        seats: EnumMap::from_fn(|color| {
                            (color == current).then(|| Seat {
                                peer_id: PeerId::new("me"),
                                nickname: "Me".into(),
                            })
                        }),
                        participants: vec![],
                    },
                    last_move: None,
                };
                state
            });
            use_context_provider(|| state);
            let (tx, rx) = use_hook(|| {
                let (tx, rx) = unbounded();
                (tx, std::rc::Rc::new(std::cell::RefCell::new(rx)))
            });
            use_context_provider(|| rx);
            let sender = use_signal(|| Some(CommandSender::new(tx)));
            super::use_online_game_ctx(state, sender);
            rsx! {}
        });
        dom.rebuild_in_place();
        settle(&mut dom);
        dom.in_scope(ScopeId::APP, || {
            let ctx = consume_context::<GameCtx>();
            let state = consume_context::<Store<OnlineState>>();
            assert!(ctx.driver.controls(ctx.game_state.read().current_player()));
            ctx.ui_state.zoom().set(1.5);
            ctx.ui_state.pan_x().set(80.0);
            ctx.ui_state.pan_y().set(-35.0);
            let color = ctx.game_state.read().current_player();
            press_tile(
                ctx,
                color,
                0,
                Pointer::new(1, 20.0, 20.0),
                Rect::new(0.0, 0.0, 50.0, 60.0),
                5.0,
            );
            move_tile(ctx, Pointer::new(1, 60.0, 60.0));
            assert!(ctx
                .ui_state
                .tile_drag()
                .peek()
                .is_some_and(|drag| drag.is_dragging()));
            ctx.ui_state.selected_tile().set(Some(0));
            ctx.ui_state.selected_position().set(Some(Position::CENTER));
            ctx.ui_state.phase().set(TurnPhase::SelectRotation);
            state
                .connection()
                .set(ConnectionStatus::Reconnecting { attempt: 1 });
        });
        settle(&mut dom);
        dom.in_scope(ScopeId::APP, || {
            let ctx = consume_context::<GameCtx>();
            let color = ctx.game_state.read().current_player();
            assert!(!ctx.driver.controls(color));
            assert!(!ctx.driver.can_end_game());
            assert!(ctx.ui_state.tile_drag().peek().is_none());
            assert!(ctx.ui_state.selected_tile().peek().is_none());
            assert!(ctx.ui_state.selected_position().peek().is_none());
            assert_eq!(*ctx.ui_state.phase().peek(), TurnPhase::SelectTile);
            assert_eq!(*ctx.ui_state.zoom().peek(), 1.5);
            assert_eq!(*ctx.ui_state.pan_x().peek(), 80.0);
            assert_eq!(*ctx.ui_state.pan_y().peek(), -35.0);
            actions::click_tile(ctx, color, 0);
            actions::click_position(ctx, Position::CENTER);
            actions::execute_move(ctx, 0, Position::CENTER, Rotation::None);
            actions::end_game(ctx);
            let commands = consume_context::<
                std::rc::Rc<
                    std::cell::RefCell<
                        futures_channel::mpsc::UnboundedReceiver<tantrix_net::SessionCommand>,
                    >,
                >,
            >();
            assert!(commands.borrow_mut().try_recv().is_err());
        });
    }
}
