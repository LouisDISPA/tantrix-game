//! Online game view — reuses the local `GamePlay` screen by providing a
//! `GameCtx` whose `game_state` is rebuilt from every incoming
//! `GameSnapshot`, and whose `online` sender is wired to the session.

use dioxus::prelude::*;

use crate::hooks::{use_initial_snapshot, use_online_game_ctx};
use crate::pages::local_game::GamePlay;
use crate::pages::online::hud_badge::OnlineHudBadge;
use crate::pages::online::participants_drawer::ParticipantsDrawer;
use crate::pages::online::screens::WaitingForHost;
use crate::state::online_state::{CommandSender, OnlineState};

#[component]
pub fn OnlineGameView(
    state: Store<OnlineState>,
    sender: Signal<Option<CommandSender>>,
) -> Element {
    let Some(initial) = use_initial_snapshot(state) else {
        return rsx! { WaitingForHost {} };
    };

    let ctx = use_online_game_ctx(state, sender, initial);
    use_context_provider(|| ctx);

    rsx! {
        GamePlay {}
        OnlineHudBadge { state }
        ParticipantsDrawer { state, sender }
    }
}
