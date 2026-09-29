//! Shared helpers for opening a matchbox socket and waiting for the
//! signaling server to assign a peer id. Used by both `host` and `client`.

use std::time::Duration;

use futures::future::{Fuse, FutureExt};
use futures::select;
use futures_channel::mpsc::UnboundedSender;
use matchbox_socket::{
    ChannelConfig, MessageLoopFuture, PeerId as MatchboxPeerId, WebRtcChannel, WebRtcSocket,
    WebRtcSocketBuilder,
};

use crate::session::SessionEvent;

/// Create a single-reliable-channel socket for `room_url` and take ownership
/// of channel 0 up front so the caller can `.next()` it directly in a
/// `select!` without re-borrowing through the socket.
pub(crate) fn open(room_url: String) -> (WebRtcSocket, WebRtcChannel, Fuse<MessageLoopFuture>) {
    let (mut socket, loop_fut) = WebRtcSocketBuilder::new(room_url)
        .add_channel(ChannelConfig::reliable())
        .build();
    let channel = socket
        .take_channel(0)
        .expect("just-added channel must be takeable");
    (socket, channel, loop_fut.fuse())
}

/// Poll `socket.id()` until it becomes `Some`, or the signaling loop
/// terminates. On terminal failure, emits a `Disconnected` event and
/// returns `None`.
pub(crate) async fn wait_for_peer_id(
    socket: &mut WebRtcSocket,
    loop_fut: &mut Fuse<MessageLoopFuture>,
    events_tx: &UnboundedSender<SessionEvent>,
    pre_id_reason: &str,
) -> Option<MatchboxPeerId> {
    loop {
        if let Some(id) = socket.id() {
            return Some(id);
        }
        select! {
            res = &mut *loop_fut => {
                let _ = events_tx.unbounded_send(SessionEvent::Disconnected {
                    reason: match res {
                        Ok(()) => pre_id_reason.to_string(),
                        Err(e) => format!("signaling error: {e}"),
                    },
                });
                return None;
            }
            _ = futures_timer::Delay::new(Duration::from_millis(50)).fuse() => {}
        }
    }
}
