//! Helpers shared by the room test suites.

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use tantrix_proto::PeerMessage;
use tantrix_room::engine::Effect;
use tantrix_room::session_cookie::SessionToken;

/// The fake hosts never wait, so one poll finishes a call.
pub fn now<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("the fake host never waits"),
    }
}

pub fn token(name: &str) -> SessionToken {
    SessionToken::new(format!("secret-{name}-0123456789"))
}

pub fn hello(nickname: &str) -> PeerMessage {
    PeerMessage::Hello {
        nickname: nickname.into(),
    }
}

/// The sockets `effects` close, with their reasons.
pub fn closes(effects: &[Effect]) -> Vec<(u64, &'static str)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Close { conn, reason } => Some((conn.0, *reason)),
            _ => None,
        })
        .collect()
}
