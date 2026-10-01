//! Online game view — shows the shared game screens with a `GameCtx` whose
//! `game_state` is rebuilt from every incoming `GameSnapshot`, and whose
//! driver sends this device's moves to the room.

use dioxus::prelude::*;

use crate::hooks::use_online_game_ctx;
use crate::pages::game::{GameOver, GamePlay};
use crate::pages::online::hud_badge::OnlineHudBadge;
use crate::pages::online::seat_takeover::OnlineSeatTakeover;
use crate::route::Route;
use crate::state::online_state::{CommandSender, OnlineState};
use crate::state::online_state::{OnlinePhase, OnlineStateStoreExt};

#[component]
pub fn OnlineGameView(state: Store<OnlineState>, sender: Signal<Option<CommandSender>>) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("OnlineGameView", ());
    // The room mounts this view in the Playing and Ended phases, which both
    // carry a snapshot. Which screen to show is a memo over the phase: this
    // view re-renders when the game ends, not on every snapshot (the game
    // context fans those out to the components that show them).
    let ended = use_memo(move || matches!(&*state.phase().read(), OnlinePhase::Ended { .. }));
    use_online_game_ctx(state, sender);

    if ended() {
        let nav = navigator();
        return rsx! {
            GameOver {
                again_label: "Online Menu",
                on_again: move |_| {
                    let _ = nav.push(Route::OnlineEntry {});
                },
                OnlineHudBadge { state }
            }
        };
    }

    rsx! {
        GamePlay {}
        OnlineSeatTakeover { state, sender }
        OnlineHudBadge { state }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;
    use tantrix_game::{Color, EnumMap};
    use tantrix_proto::GameSnapshot;

    use super::OnlineGameView;
    use crate::pages::game::rerender_tests::settle;
    use crate::state::config::GameConfig;
    use crate::state::game_state::GameState;
    use crate::state::online_state::{OnlinePhase, OnlineState, OnlineStateStoreExt};
    use crate::util::render_count;

    /// What a room would send for `game` (watched: no seats).
    fn snapshot_of(game: &GameState) -> GameSnapshot {
        GameSnapshot {
            board: game.board().clone(),
            hands: EnumMap::from_fn(|color| game.hand(color).clone()),
            bag_count: game.bag_count(),
            current_player: game.current_player(),
            turn_step: game.turn_step(),
            playing_colors: game.playing_colors().to_vec(),
            seats: EnumMap::default(),
            participants: Vec::new(),
        }
    }

    #[test]
    fn a_room_snapshot_re_renders_only_what_it_changes() {
        // A three-player room, one move apart.
        let mut config = GameConfig::new(Some(3));
        config.cycle_player(Color::Yellow);
        let mut game = GameState::new(&config);
        let before = snapshot_of(&game);
        let mover = game.current_player();
        let m = game.legal_moves()[0];
        game.play_move(m.placement()).unwrap();
        let next = game.current_player();
        let after = snapshot_of(&game);

        let mut dom = VirtualDom::new_with_props(
            |snapshot: GameSnapshot| {
                let state = use_store(move || OnlineState {
                    room_code: "ABCDEF".to_string(),
                    my_peer_id: None,
                    phase: OnlinePhase::Playing {
                        snapshot,
                        last_move: None,
                    },
                    last_error: None,
                });
                use_context_provider(|| state);
                let sender = use_signal(|| None);
                rsx! { OnlineGameView { state, sender } }
            },
            before,
        );
        dom.rebuild_in_place();
        settle(&mut dom);

        render_count::reset();
        dom.in_scope(ScopeId::APP, || {
            consume_context::<Store<OnlineState>>()
                .phase()
                .set(OnlinePhase::Playing {
                    snapshot: after,
                    last_move: Some(m),
                });
        });
        settle(&mut dom);

        // The move shows up on the board and in the hands it changes…
        assert_eq!(render_count::of("BoardTile", m.position), 1);
        assert_eq!(render_count::of("PlayerHand", mover), 1);
        assert_eq!(render_count::of("PlayerHand", next), 1);
        // …but the view itself, the third hand and the camera stay as they
        // were: the snapshot rebuilt the whole game, not what they show.
        assert_eq!(render_count::total("OnlineGameView"), 0);
        for bystander in [Color::Red, Color::Blue, Color::Yellow] {
            if bystander != mover && bystander != next {
                assert_eq!(render_count::of("PlayerHand", bystander), 0, "{bystander}");
            }
        }
        assert_eq!(render_count::total("ZoomControls"), 0);
        assert_eq!(render_count::total("RotationWheel"), 0);
    }
}
