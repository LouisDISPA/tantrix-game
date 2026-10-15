//! App-lifetime update state: checking/downloading survives menu navigation.

use dioxus::prelude::*;
use futures::{FutureExt, StreamExt};

use crate::state::desktop_updates::{self, UpdateStatus};

#[derive(Clone, Copy)]
pub struct DesktopUpdates {
    pub status: ReadSignal<UpdateStatus>,
    commands: Coroutine<()>,
}

impl DesktopUpdates {
    pub fn install(self) {
        if matches!(&*self.status.peek(), UpdateStatus::Ready { .. }) {
            self.commands.send(());
        }
    }
}

pub fn use_desktop_updates() {
    let mut status = use_signal(|| UpdateStatus::Checking);
    let commands = use_coroutine(move |mut commands: UnboundedReceiver<()>| async move {
        let (worker_tx, worker_rx) = std::sync::mpsc::channel();
        let (events_tx, mut events_rx) = futures_channel::mpsc::unbounded();
        if let Err(error) = std::thread::Builder::new()
            .name("tantrix-updater".into())
            .spawn(move || desktop_updates::run(worker_rx, events_tx))
        {
            log::warn!("Could not start updater: {error}");
            status.set(UpdateStatus::Manual);
            return;
        }
        loop {
            futures::select! {
                event = events_rx.next().fuse() => match event {
                    Some(next) => status.set(next),
                    None => break,
                },
                command = commands.next().fuse() => match command {
                    Some(command) => { if worker_tx.send(command).is_err() { break; } },
                    None => break,
                },
            }
        }
    });
    use_context_provider(|| DesktopUpdates {
        status: status.into(),
        commands,
    });
}
