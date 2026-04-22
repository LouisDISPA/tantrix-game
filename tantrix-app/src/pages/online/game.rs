//! Online game view — reuses the local `GamePlay` screen by providing a
//! `GameCtx` whose `game_state` is rebuilt from every incoming
//! `GameSnapshot`, and whose `online` sender is wired to the session.
//!
//! There is no separate header strip here: the HUD chip rendered by
//! `StatusBar` picks up the live connection + room info automatically via
//! the `OnlineCtx` that `OnlineRoom` has already placed in scope.

use dioxus::prelude::*;
use tantrix_game::{Bag, Board, Color, EnumMap, Player, TurnTracker};
use tantrix_proto::{GameSnapshot, Seat};

use crate::pages::local_game::GamePlay;
use crate::state::game_state::{GameCtx, GameState};
use crate::state::online_state::{OnlineCtx, OnlinePhase};
use crate::state::ui_state::{Screen, TurnPhase, UiState};

/// Inner view — the outer `OnlineRoom` has already provisioned `OnlineCtx`.
#[component]
pub fn OnlineGameView() -> Element {
    let online_ctx = use_context::<OnlineCtx>();

    // Seed the shared GameCtx from the first snapshot we see.
    let initial = {
        let s = online_ctx.state.read();
        match &s.phase {
            OnlinePhase::Playing { snapshot, .. } => snapshot.clone(),
            OnlinePhase::Ended { snapshot, .. } => snapshot.clone(),
            _ => return rsx! {
                div { class: "title-screen",
                    div { class: "title-content",
                        p { class: "title-subtitle", "Waiting for host…" }
                    }
                }
            },
        }
    };

    let game_state = use_signal(|| build_game_state(&initial));
    let screen = use_signal(|| Screen::Playing);
    let ui_state = use_store(UiState::default);
    let online = use_signal(|| online_ctx.sender.read().clone());

    let initial_my_color = online_ctx.state.read().my_color();
    let my_color = use_signal(|| initial_my_color);
    let player_names = use_signal(|| names_from_seats(&initial.seats));

    use_context_provider(|| GameCtx {
        screen,
        game_state,
        ui_state,
        online,
        my_color,
        player_names,
    });

    // Keep the `online` signal fresh if the sender appears after first paint.
    {
        let mut online = online;
        let sender = online_ctx.sender;
        use_effect(move || {
            online.set(sender.read().clone());
        });
    }

    // Keep `my_color` fresh with the seat assignment in the online state.
    {
        let mut my_color = my_color;
        let state = online_ctx.state;
        use_effect(move || {
            my_color.set(state.read().my_color());
        });
    }

    // Pipe snapshot updates into the shared GameState + player_names signals
    // and reset UI phase when a new game actually starts.
    {
        let mut game_state = game_state;
        let mut ui_state = ui_state;
        let mut player_names = player_names;
        let state = online_ctx.state;
        use_effect(move || {
            let s = state.read();
            match &s.phase {
                OnlinePhase::Playing { snapshot, last_move } => {
                    *game_state.write() = build_game_state(snapshot);
                    player_names.set(names_from_seats(&snapshot.seats));
                    if let Some(m) = last_move {
                        ui_state.write().last_placed_position = Some(m.position);
                    }
                    let mut ui = ui_state.write();
                    if ui.phase == TurnPhase::GameOver {
                        ui.phase = TurnPhase::SelectTile;
                    }
                }
                OnlinePhase::Ended { snapshot, .. } => {
                    *game_state.write() = build_game_state(snapshot);
                    player_names.set(names_from_seats(&snapshot.seats));
                    ui_state.write().phase = TurnPhase::GameOver;
                }
                _ => {}
            }
        });
    }

    rsx! { GamePlay {} }
}

fn names_from_seats(seats: &EnumMap<Color, Option<Seat>>) -> EnumMap<Color, Option<String>> {
    let mut out: EnumMap<Color, Option<String>> = EnumMap::default();
    for (c, slot) in seats.iter() {
        out[c] = slot.as_ref().map(|s| s.nickname.clone());
    }
    out
}

/// Reconstruct a local `GameState` from a remote `GameSnapshot`. Bag
/// contents are stubbed (only the count matters to the UI); player kinds
/// are all `Someone` for seated colors and `None` otherwise.
fn build_game_state(snap: &GameSnapshot) -> GameState {
    let mut players: EnumMap<Color, Player> = EnumMap::from_fn(|_| Player::None);
    for (c, slot) in snap.seats.iter() {
        players[c] = match slot {
            Some(_) => Player::Someone,
            None => Player::None,
        };
    }

    let turn_tracker =
        TurnTracker::from_parts(snap.current_player, snap.playing_colors.iter().copied());
    let bag = Bag::with_count(snap.bag_count);

    let hands = snap.hands.clone();
    let board: Board = snap.board.clone();

    let mut state = GameState {
        board,
        bag,
        hands,
        players,
        turn_tracker,
        hand_info: Vec::new(),
    };
    state.refresh_hand_info();
    state
}
