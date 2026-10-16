//! Quiet until an update is known. Online incompatibility can request a
//! fallback action while reusing the single startup check.

use dioxus::prelude::*;

use crate::hooks::desktop_updates::DesktopUpdates;
use crate::state::desktop_updates::{UpdateStatus, DOWNLOAD_URL};

#[component]
pub fn DesktopUpdate(#[props(default)] required: bool) -> Element {
    let updates = use_context::<DesktopUpdates>();
    rsx! {
        if required || updates.status.read().is_outdated() {
            section { class: "desktop-update", aria_label: "App updates",
                match &*updates.status.read() {
                    UpdateStatus::Downloading { version, percent } => rsx! {
                        div { class: "surface surface-inset desktop-update-badge", role: "status",
                            span { "Update available" }
                            span { class: "desktop-update-meta", "Downloading v{version} · {percent}%" }
                            progress { max: "100", value: "{percent}", aria_label: "Update download" }
                        }
                    },
                    UpdateStatus::Ready { version } => rsx! {
                        button {
                            class: "button button-secondary desktop-update-badge",
                            title: "Restart to install Tantrix v{version}",
                            onclick: move |_| updates.install(),
                            span { aria_hidden: "true", "↑" }
                            "Restart and update"
                            span { class: "desktop-update-meta", "v{version}" }
                        }
                    },
                    UpdateStatus::Installing => rsx! {
                        button { class: "button button-secondary desktop-update-badge", disabled: true, "Restarting…" }
                    },
                    UpdateStatus::Checking => rsx! {
                        p { class: "desktop-update-meta", role: "status", "Checking for updates…" }
                    },
                    status => rsx! {
                        a {
                            class: "button button-secondary desktop-update-badge",
                            href: DOWNLOAD_URL,
                            target: "_blank",
                            rel: "noopener noreferrer",
                            if status.is_outdated() {
                                "Update available · Download ↗"
                            } else {
                                "Download latest ↗"
                            }
                        }
                    },
                }
            }
        }
    }
}
