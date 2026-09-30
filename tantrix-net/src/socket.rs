//! Shared helpers for opening a matchbox socket and waiting for the
//! signaling server to assign a peer id. Used by both `host` and `client`.

use std::time::Duration;

use futures::future::{Fuse, FutureExt};
use futures::select;
use futures_channel::mpsc::UnboundedSender;
use log::{info, warn};
use matchbox_socket::{
    ChannelConfig, MessageLoopFuture, PeerId as MatchboxPeerId, WebRtcChannel, WebRtcSocket,
    WebRtcSocketBuilder,
};

use crate::session::{ConnectStage, SessionEvent};

/// How long the signaling server gets to hand us a peer id.
pub(crate) const SIGNALING_TIMEOUT: Duration = Duration::from_secs(15);

/// How long a client waits for the WebRTC link to the host after signaling.
///
/// A healthy handshake takes well under two seconds. The bulk of a slow one is
/// ICE gathering, which the patched `matchbox_socket` caps at three seconds on
/// *each* side (offerer, then answerer), plus a few seconds of connectivity
/// checks — so this leaves room for a slow-but-working network.
pub(crate) const PEER_LINK_TIMEOUT: Duration = Duration::from_secs(15);

/// How long a client waits for `Welcome` once the data channel is open.
pub(crate) const WELCOME_TIMEOUT: Duration = Duration::from_secs(8);

/// Create a single-reliable-channel socket for `room_url` and take ownership
/// of channel 0 up front so the caller can `.next()` it directly in a
/// `select!` without re-borrowing through the socket.
pub(crate) fn open(room_url: String) -> (WebRtcSocket, WebRtcChannel, Fuse<MessageLoopFuture>) {
    info!("net: opening signaling socket {room_url}");
    let (mut socket, loop_fut) = WebRtcSocketBuilder::new(room_url)
        .add_channel(ChannelConfig::reliable())
        .build();
    let channel = socket
        .take_channel(0)
        .expect("just-added channel must be takeable");
    (socket, channel, loop_fut.fuse())
}

/// Poll `socket.id()` until it becomes `Some`, the signaling loop terminates,
/// or [`SIGNALING_TIMEOUT`] passes. On failure, emits a terminal event and
/// returns `None`.
pub(crate) async fn wait_for_peer_id(
    socket: &mut WebRtcSocket,
    loop_fut: &mut Fuse<MessageLoopFuture>,
    events_tx: &UnboundedSender<SessionEvent>,
    pre_id_reason: &str,
) -> Option<MatchboxPeerId> {
    let mut deadline = futures_timer::Delay::new(SIGNALING_TIMEOUT).fuse();
    loop {
        if let Some(id) = socket.id() {
            info!("net: signaling assigned peer id {id}");
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
            _ = deadline => {
                warn!("net: no peer id from the signaling server after {SIGNALING_TIMEOUT:?}");
                let _ = events_tx.unbounded_send(SessionEvent::ConnectTimeout {
                    stage: ConnectStage::Signaling,
                });
                return None;
            }
            _ = futures_timer::Delay::new(Duration::from_millis(50)).fuse() => {}
        }
    }
}
