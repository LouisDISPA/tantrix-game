#![warn(missing_docs)]
// Upstream 0.14.0 uses `try_next`, deprecated by newer futures-channel. Silenced
// here because a path dependency (unlike a registry one) surfaces the warning.
#![allow(deprecated)]
#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

mod error;
#[cfg(feature = "ggrs")]
mod ggrs_socket;
mod webrtc_socket;

pub use async_trait;
pub use error::{Error, SignalingError};
pub use matchbox_protocol::PeerId;
pub use webrtc_socket::{
    ChannelConfig, MessageLoopFuture, Packet, PeerEvent, PeerRequest, PeerSignal, PeerState,
    RtcIceServerConfig, Signaller, SignallerBuilder, WebRtcChannel, WebRtcSocket,
    WebRtcSocketBuilder, error::ChannelError,
};
