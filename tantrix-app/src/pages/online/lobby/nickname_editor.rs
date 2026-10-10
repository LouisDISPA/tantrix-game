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
        if trimmed.is_empty() || trimmed == *authoritative.peek() {
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
            max_len: nickname::MAX_CHARS,
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

#[cfg(test)]
mod tests {
    use tantrix_game::Color::Red;
    use tantrix_proto::PeerId;

    use super::authoritative_nickname;
    use crate::state::online_state::test_rooms::{lobby, snapshot};
    use crate::state::online_state::OnlinePhase;

    #[test]
    fn the_name_to_edit_is_the_one_the_room_knows() {
        let seated = [(Red, "Ann", true)];
        let phase = OnlinePhase::Lobby {
            lobby: lobby(&seated, &["Bob"]),
        };
        let name = |peer: &str| authoritative_nickname(&phase, Some(&PeerId::new(peer)));
        assert_eq!(name("Ann").as_deref(), Some("Ann"));
        assert_eq!(name("Bob").as_deref(), Some("Bob"));
        assert_eq!(name("Cy"), None);
        // Before the welcome, and once the game is on, there is none.
        assert_eq!(authoritative_nickname(&phase, None), None);
        let playing = OnlinePhase::Playing {
            snapshot: snapshot(&seated, &[]),
            last_move: None,
        };
        assert_eq!(
            authoritative_nickname(&playing, Some(&PeerId::new("Ann"))),
            None
        );
    }
}
