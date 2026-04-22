//! Online game view — reuses the local `GamePlay` screen by providing a
//! `GameCtx` whose `game_state` is rebuilt from every incoming
//! `GameSnapshot`, and whose `online` sender is wired to the session.

use dioxus::prelude::*;
use tantrix_game::{Bag, Board, Color, EnumMap, Player, TurnTracker};
use tantrix_proto::{EndReason, GameSnapshot};

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

    use_context_provider(|| GameCtx { screen, game_state, ui_state, online });

    // Keep the `online` signal fresh if the sender appears after first paint.
    {
        let mut online = online;
        let sender = online_ctx.sender;
        use_effect(move || {
            online.set(sender.read().clone());
        });
    }

    // Pipe snapshot updates into the shared GameState signal and reset UI
    // phase when a new game actually starts.
    {
        let mut game_state = game_state;
        let mut ui_state = ui_state;
        let state = online_ctx.state;
        use_effect(move || {
            let s = state.read();
            match &s.phase {
                OnlinePhase::Playing { snapshot, last_move } => {
                    *game_state.write() = build_game_state(snapshot);
                    if let Some(m) = last_move {
                        let writer = ui_state.write();
                        drop(writer);
                        let mut ui = ui_state.write();
                        ui.last_placed_position = Some(m.position);
                    }
                    let mut ui = ui_state.write();
                    if ui.phase == TurnPhase::GameOver {
                        ui.phase = TurnPhase::SelectTile;
                    }
                }
                OnlinePhase::Ended { snapshot, .. } => {
                    *game_state.write() = build_game_state(snapshot);
                    ui_state.write().phase = TurnPhase::GameOver;
                }
                _ => {}
            }
        });
    }

    let state = online_ctx.state.read();
    let error = state.last_error.clone();
    let header = header_text(&state);
    drop(state);

    rsx! {
        div { class: "online-game-wrap",
            div { class: "online-game-header",
                span { class: "online-game-status", "{header}" }
                if let Some(err) = error {
                    span { class: "online-error", "{err}" }
                }
            }
            GamePlay {}
        }
    }
}

fn header_text(state: &crate::state::online_state::OnlineState) -> String {
    let mine = state
        .my_color()
        .map(color_name)
        .unwrap_or("spectator");
    match &state.phase {
        OnlinePhase::Playing { snapshot, .. } => {
            let turn = color_name(snapshot.current_player);
            format!("Room {} · You are {} · {}'s turn", state.room_code, mine, turn)
        }
        OnlinePhase::Ended { reason, .. } => {
            let ended = match reason {
                EndReason::Completed { winners } => {
                    if winners.is_empty() {
                        "Game over".to_string()
                    } else if winners.len() == 1 {
                        format!("{} wins", color_name(winners[0]))
                    } else {
                        let names: Vec<&str> = winners.iter().copied().map(color_name).collect();
                        format!("Tie: {}", names.join(" / "))
                    }
                }
                EndReason::PlayerDisconnected { color } => {
                    format!("{} disconnected — game over", color_name(*color))
                }
                EndReason::HostLeft => "Host left — game over".into(),
            };
            format!("Room {} · {}", state.room_code, ended)
        }
        _ => format!("Room {}", state.room_code),
    }
}

fn color_name(c: Color) -> &'static str {
    match c {
        Color::Red => "Red",
        Color::Blue => "Blue",
        Color::Yellow => "Yellow",
        Color::Green => "Green",
    }
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

    // Reuse Hand clones from the snapshot.
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
