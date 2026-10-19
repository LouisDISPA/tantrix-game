use crate::hooks::android_updates::{AndroidUpdates, DownloadStatus};
use dioxus::prelude::*;

#[component]
pub fn AndroidUpdate(#[props(default)] required: bool) -> Element {
    let mut updates = use_context::<AndroidUpdates>();
    rsx! {
        match &*updates.status.read() {
            DownloadStatus::Downloading(percent) => rsx! {
                div { class: "desktop-update stack gap-2", role: "status",
                    "Downloading update · {percent}%"
                    progress { max: "100", value: "{percent}", aria_label: "Update download" }
                }
            },
            DownloadStatus::Ready { message, .. } => rsx! {
                button {
                    class: "button button-secondary desktop-update-badge",
                    disabled: updates.is_busy(),
                    onclick: move |_| updates.install(),
                    "Install update"
                }
                if let Some(message) = message {
                    p { role: "status", "{message}" }
                }
            },
            status => rsx! {
                if let Some(Ok(Some(update))) = &*updates.release.read() {
                    button {
                        class: "button button-secondary desktop-update-badge",
                        disabled: updates.is_busy(),
                        onclick: move |_| updates.install(),
                        "Download and install v{update.version}"
                    }
                } else if required {
                    button {
                        class: "button button-secondary",
                        disabled: updates.release.state()() != UseResourceState::Ready,
                        onclick: move |_| updates.release.restart(),
                        "Check for updates"
                    }
                    if let Some(Err(_)) = &*updates.release.read() {
                        p { role: "alert", "Couldn’t check for updates. Try again." }
                    } else if let Some(Ok(None)) = &*updates.release.read() {
                        p { role: "status", "No newer Android release available." }
                    }
                }
                if let DownloadStatus::Failed(error) = status {
                    p { role: "alert", "{error}" }
                }
            },
        }
    }
}
