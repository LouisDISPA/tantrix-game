//! Bridge a snapshot stream from `Store<OnlineState>` into the shared
//! `GameCtx` used by the (local) `GamePlay` screen.
//!
//! The hook owns the `GameCtx` signals and keeps them in sync with the
//! online state via two small effects:
//!
//! 1. Mirror the session command sender into `GameCtx::online`.
//! 2. Fan out each inbound snapshot into `game_state` / `player_names` /
//!    `my_color` / `ui_state`.

use dioxus::prelude::*;
use tantrix_game::{Color, EnumMap};
use tantrix_proto::{GameSnapshot, Seat};

use crate::state::game_state::{GameCtx, GameState};
use crate::state::online_state::{
    queries, CommandSender, OnlinePhase, OnlineState, OnlineStateStoreExt,
};
use crate::state::ui_state::{Screen, TurnPhase, UiState};

/// Build the `GameCtx` for an online game, seeded from `initial` and kept
/// live via reactive effects on the surrounding online store + sender.
pub fn use_online_game_ctx(
    state: Store<OnlineState>,
    sender: Signal<Option<CommandSender>>,
    initial: GameSnapshot,
) -> GameCtx {
    let game_state = use_signal(|| GameState::from_snapshot(&initial));
    let screen = use_signal(|| Screen::Playing);
    let ui_state = use_store(UiState::default);
    let online = use_signal(|| sender.read().clone());
    let my_color = use_signal(|| queries::my_color_of(state));
    let player_names = use_signal(|| names_from_seats(&initial.seats));

    mirror_sender(online, sender);
    fan_out_snapshots(state, game_state, ui_state, player_names, my_color);

    GameCtx {
        screen,
        game_state,
        ui_state,
        online,
        my_color,
        player_names,
    }
}

/// Read the Playing/Ended snapshot out of the online state, if any. Returns
/// `None` while the server hasn't sent `GameStarted` yet.
pub fn use_initial_snapshot(state: Store<OnlineState>) -> Option<GameSnapshot> {
    let phase = state.phase();
    let phase = phase.read();
    match &*phase {
        OnlinePhase::Playing { snapshot, .. } => Some(snapshot.clone()),
        OnlinePhase::Ended { snapshot, .. } => Some(snapshot.clone()),
        _ => None,
    }
}

fn mirror_sender(online: Signal<Option<CommandSender>>, sender: Signal<Option<CommandSender>>) {
    let mut online = online;
    use_effect(move || {
        online.set(sender.read().clone());
    });
}

fn fan_out_snapshots(
    state: Store<OnlineState>,
    mut game_state: Signal<GameState>,
    mut ui_state: Store<UiState>,
    mut player_names: Signal<EnumMap<Color, Option<String>>>,
    mut my_color: Signal<Option<Color>>,
) {
    use_effect(move || {
        let phase = state.phase();
        let phase = phase.read();

        let new_color = queries::my_color_of(state);
        if *my_color.peek() != new_color {
            my_color.set(new_color);
        }

        let (snapshot, last_move, terminal) = match &*phase {
            OnlinePhase::Playing {
                snapshot,
                last_move,
            } => (snapshot, last_move.as_ref(), false),
            OnlinePhase::Ended { snapshot, .. } => (snapshot, None, true),
            _ => return,
        };

        *game_state.write() = GameState::from_snapshot(snapshot);

        let fresh_names = names_from_seats(&snapshot.seats);
        if *player_names.peek() != fresh_names {
            player_names.set(fresh_names);
        }

        let mut ui = ui_state.write();
        if terminal {
            ui.phase = TurnPhase::GameOver;
            ui.selected_tile = None;
            ui.selected_position = None;
            ui.hovered = crate::state::ui_state::Hovered::None;
        } else {
            ui.reset_selection();
            if let Some(m) = last_move {
                ui.last_placed_position = Some(m.position);
            }
        }
    });
}

fn names_from_seats(seats: &EnumMap<Color, Option<Seat>>) -> EnumMap<Color, Option<String>> {
    EnumMap::from_fn(|c| seats[c].as_ref().map(|s| s.nickname.clone()))
}
