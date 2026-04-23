//! Cloneable handle to the session's command sender.
//!
//! Wrapped so the `Signal` type parameter used by the UI stays small and
//! `Copy`, and to centralize the "dropped command" log policy.

use futures_channel::mpsc::UnboundedSender;
use tantrix_net::SessionCommand;

#[derive(Clone)]
pub struct CommandSender(UnboundedSender<SessionCommand>);

impl CommandSender {
    pub fn new(tx: UnboundedSender<SessionCommand>) -> Self {
        Self(tx)
    }

    /// Dispatch a command to the session task. A send failure means the
    /// driver has already exited — the next `SessionEvent::Disconnected`
    /// will push us to `OnlinePhase::Failed` one tick later, so the user
    /// still sees a terminal screen. We log here to surface the narrow
    /// race where a click lands between driver-exit and phase-update.
    pub fn send(&self, cmd: SessionCommand) {
        if let Err(err) = self.0.unbounded_send(cmd) {
            log::warn!("online: dropped command, session receiver closed: {err}");
        }
    }
}
