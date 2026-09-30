//! Online game view — reuses the local `GamePlay` screen by providing a
//! `GameCtx` whose `game_state` is rebuilt from every incoming
//! `GameSnapshot`, and whose `online` sender is wired to the session.

use dioxus::prelude::*;

use crate::components::board::Board;
use crate::components::scores_panel::ScoresPanel;
use crate::hooks::{use_initial_snapshot, use_online_game_ctx};
use crate::pages::local_game::GamePlay;
use crate::pages::online::hud_badge::OnlineHudBadge;
use crate::pages::online::seat_takeover::OnlineSeatTakeover;
use crate::pages::online::screens::WaitingForGame;
use crate::route::Route;
use crate::state::online_state::{CommandSender, OnlineState};
use crate::state::online_state::{OnlinePhase, OnlineStateStoreExt};

#[component]
pub fn OnlineGameView(state: Store<OnlineState>, sender: Signal<Option<CommandSender>>) -> Element {
    let Some(initial) = use_initial_snapshot(state) else {
        return rsx! { WaitingForGame {} };
    };

    let ctx = use_online_game_ctx(state, sender, initial);
    use_context_provider(|| ctx);

    let ended = matches!(&*state.phase().read(), OnlinePhase::Ended { .. });
    if ended {
        let nav = navigator();
        return rsx! {
            div { class: "game-layout game-over-layout",
                div { class: "board-slot", Board {} }
                ScoresPanel {
                    button {
                        class: "results-btn results-btn-primary",
                        onclick: move |_| {
                            let _ = nav.push(Route::OnlineEntry {});
                        },
                        "Online Menu"
                    }
                    button {
                        class: "results-btn results-btn-secondary",
                        onclick: move |_| {
                            let _ = nav.push(Route::MainMenu {});
                        },
                        "Main Menu"
                    }
                }
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
