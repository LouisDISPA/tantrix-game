//! Bridge between a `tantrix_net::SessionHandle` and the reactive
//! `OnlineCtx` context provided to child components.

use dioxus::prelude::*;
use futures::StreamExt;
use tantrix_net::{
    SessionEvent, SessionHandle, SessionRole, open_client_session, open_host_session,
};

use crate::state::online_state::{CommandSender, OnlineCtx, OnlinePhase, OnlineState, reject_message};
use crate::state::signaling::signaling_url;

/// Open a session (host or client) for the given room and pump events into
/// the `OnlineCtx::state` signal. The spawned tasks live as long as the
/// component is mounted.
pub fn spawn_session(
    ctx: OnlineCtx,
    room_code: String,
    role: SessionRole,
    nickname: String,
) {
    let url = signaling_url(&room_code);

    let (handle, run_fut): (SessionHandle, _) = match role {
        SessionRole::Host => {
            let (h, f) = open_host_session(url, nickname);
            (h, futures::future::Either::Left(f))
        }
        SessionRole::Client => {
            let (h, f) = open_client_session(url, nickname);
            (h, futures::future::Either::Right(f))
        }
    };

    // Publish the command sender so child components can dispatch actions.
    let mut sender = ctx.sender;
    sender.set(Some(CommandSender::new(handle.commands)));

    // Drive the network task.
    spawn(async move {
        run_fut.await;
    });

    // Pipe events into state.
    let mut events = handle.events;
    let mut state = ctx.state;
    spawn(async move {
        while let Some(event) = events.next().await {
            apply_event(&mut state, event);
        }
    });
}

fn apply_event(state: &mut Signal<OnlineState>, event: SessionEvent) {
    let mut s = state.write();
    match event {
        SessionEvent::Connected {
            role: _,
            my_peer_id,
            host_peer_id,
        } => {
            s.my_peer_id = Some(my_peer_id);
            s.host_peer_id = Some(host_peer_id);
        }
        SessionEvent::Lobby(lobby) => {
            s.phase = OnlinePhase::Lobby { lobby };
            s.last_error = None;
        }
        SessionEvent::GameStarted { snapshot } => {
            s.phase = OnlinePhase::Playing {
                snapshot,
                last_move: None,
            };
            s.last_error = None;
        }
        SessionEvent::StateUpdate { snapshot, last_move } => {
            s.phase = OnlinePhase::Playing { snapshot, last_move };
        }
        SessionEvent::GameEnded { reason: _, snapshot } => {
            s.phase = OnlinePhase::Ended { snapshot };
        }
        SessionEvent::Error { code, message } => {
            let friendly = code
                .as_ref()
                .map(|c| reject_message(c).to_string())
                .unwrap_or(message);
            s.last_error = Some(friendly);
        }
        SessionEvent::Disconnected { reason } => {
            s.phase = OnlinePhase::Failed { reason };
        }
    }
}
