//! Online game view — shows the shared game screens with a `GameCtx` whose
//! `game_state` is rebuilt from every incoming `GameSnapshot`, and whose
//! driver sends this device's moves to the room.

use dioxus::prelude::*;

use crate::hooks::{use_initial_snapshot, use_online_game_ctx};
use crate::pages::game::{GameOver, GamePlay};
use crate::pages::online::hud_badge::OnlineHudBadge;
use crate::pages::online::screens::WaitingForGame;
use crate::pages::online::seat_takeover::OnlineSeatTakeover;
use crate::route::Route;
use crate::state::online_state::{CommandSender, OnlineState};
use crate::state::online_state::{OnlinePhase, OnlineStateStoreExt};

#[component]
pub fn OnlineGameView(state: Store<OnlineState>, sender: Signal<Option<CommandSender>>) -> Element {
    let Some(initial) = use_initial_snapshot(state) else {
        return rsx! { WaitingForGame {} };
    };

    use_online_game_ctx(state, sender, initial);

    let ended = matches!(&*state.phase().read(), OnlinePhase::Ended { .. });
    if ended {
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
