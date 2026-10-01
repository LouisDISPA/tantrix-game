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
