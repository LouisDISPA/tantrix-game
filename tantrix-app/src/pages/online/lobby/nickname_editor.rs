//! Store-aware glue around [`NicknameInput`]: derives the authoritative
//! nickname from the online state and dispatches `UpdateNickname` +
//! persists to localStorage on commit.

use dioxus::prelude::*;
use tantrix_net::SessionCommand;
use tantrix_proto::PeerId;

use crate::components::nickname_input::NicknameInput;
use crate::state::nickname;
use crate::state::online_state::{CommandSender, OnlinePhase, OnlineState, OnlineStateStoreExt};

#[component]
pub fn LobbyNicknameEditor(
    state: Store<OnlineState>,
    sender: Signal<Option<CommandSender>>,
) -> Element {
    let authoritative = use_memo(move || {
        let phase = state.phase();
        let phase = phase.read();
        let me = state.my_peer_id();
        let me = me.read();
        authoritative_nickname(&phase, me.as_ref()).unwrap_or_default()
    });

    let on_commit = move |text: String| {
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        if let Some(s) = sender.read().as_ref() {
            s.send(SessionCommand::UpdateNickname(trimmed.clone()));
        }
        nickname::persist(&trimmed);
    };

    rsx! {
        NicknameInput {
            authoritative,
            max_len: 24,
            on_commit,
        }
    }
}

fn authoritative_nickname(phase: &OnlinePhase, me: Option<&PeerId>) -> Option<String> {
    let me = me?;
    let lobby = match phase {
        OnlinePhase::Lobby { lobby } => lobby,
        _ => return None,
    };
    lobby
        .participants
        .iter()
        .find(|p| &p.peer_id == me)
        .map(|p| p.nickname.clone())
}
