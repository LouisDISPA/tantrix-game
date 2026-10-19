//! Update checks and downloads survive navigation for the lifetime of the app.
use crate::state::android_updates::{self, Update};
use dioxus::prelude::*;
use futures::StreamExt;
use std::path::PathBuf;

#[derive(Clone, PartialEq)]
pub enum DownloadStatus {
    Idle,
    Downloading(u8),
    Ready {
        path: PathBuf,
        message: Option<String>,
    },
    Failed(String),
}

#[derive(Clone, Copy)]
pub struct AndroidUpdates {
    pub release: Resource<Result<Option<Update>, String>>,
    pub status: ReadSignal<DownloadStatus>,
    commands: Coroutine<()>,
    busy: Signal<bool>,
}

impl AndroidUpdates {
    pub fn install(mut self) {
        if !*self.busy.peek() {
            self.busy.set(true);
            self.commands.send(());
        }
    }

    pub fn is_busy(self) -> bool {
        *self.busy.read()
    }
}

pub fn use_android_updates() {
    let release = use_resource(|| async {
        let result = android_updates::check().await;
        if let Err(error) = &result {
            log::warn!("Update check failed: {error}");
        }
        result
    });
    let mut status = use_signal(|| DownloadStatus::Idle);
    let mut busy = use_signal(|| false);
    let commands = use_coroutine(move |mut commands: UnboundedReceiver<()>| async move {
        while commands.next().await.is_some() {
            let existing = match &*status.peek() {
                DownloadStatus::Ready { path, .. } => Some(path.clone()),
                _ => None,
            };
            let path = if let Some(path) = existing {
                path
            } else {
                let update = match &*release.peek() {
                    Some(Ok(Some(update))) => update.clone(),
                    _ => {
                        busy.set(false);
                        continue;
                    }
                };
                status.set(DownloadStatus::Downloading(0));
                match android_updates::download(&update, |percent| {
                    let next = DownloadStatus::Downloading(percent);
                    if *status.peek() != next {
                        status.set(next);
                    }
                })
                .await
                {
                    Ok(path) => path,
                    Err(error) => {
                        status.set(DownloadStatus::Failed(error));
                        busy.set(false);
                        continue;
                    }
                }
            };
            let message = match android_updates::install(path.clone()).await {
                Ok(true) => None,
                Ok(false) => Some("Allow installs, then tap Install update.".into()),
                Err(error) => {
                    status.set(DownloadStatus::Failed(error));
                    busy.set(false);
                    continue;
                }
            };
            status.set(DownloadStatus::Ready { path, message });
            busy.set(false);
        }
    });
    use_context_provider(|| AndroidUpdates {
        release,
        status: status.into(),
        commands,
        busy,
    });
}
