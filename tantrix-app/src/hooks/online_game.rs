//! Bridge a snapshot stream from `Store<OnlineState>` into the `GameCtx`
//! of the shared game screens.
//!
//! The context comes from [`use_game_ctx`] with an online [`GameDriver`]:
//! moves go out through the session's command sender, and the seat this
//! device plays and the players' nicknames are memos over the online state.
//! One effect fans each inbound snapshot out into `game_state` / `ui_state`.

use dioxus::prelude::*;
use tantrix_game::{Color, EnumMap};
use tantrix_proto::{GameSnapshot, Seat};

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
    let driver = GameDriver::Online(RoomLink::new(sender, my_color, names));
    // The first snapshot is read once, without subscribing: the effect below
    // brings in every later one.
    let ctx = use_game_ctx(driver, move || {
        let phase = state.phase();
        let phase = phase.peek();
        let snapshot = snapshot_in(&phase).expect("the online game starts from a snapshot");
        GameState::from_snapshot(snapshot)
    });

    fan_out_snapshots(state, ctx.game_state, ctx.ui_state);
    ctx
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

        *game_state.write() = GameState::from_snapshot(snapshot);

        // Field by field (and never reading `ui_state`, which would make this
        // effect rerun on its own writes): a snapshot leaves the camera, the
        // animation hints and the scoring overlay alone.
        if terminal {
            ui_state.clear_selection();
            set_if_changed(ui_state.phase(), TurnPhase::GameOver);
        } else {
            ui_state.reset_selection();
            if let Some(m) = last_move {
                set_if_changed(ui_state.last_placed_position(), Some(m.position));
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

    use super::{names_from_seats, snapshot_in};
    use crate::state::online_state::test_rooms::{lobby, snapshot};
    use crate::state::online_state::{FailureReason, OnlinePhase};

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
}
