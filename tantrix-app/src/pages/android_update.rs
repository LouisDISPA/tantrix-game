use crate::hooks::android_updates::AndroidUpdates;
use crate::state::android_updates::RELEASES_URL;
use dioxus::prelude::*;

#[component]
pub fn AndroidUpdate(#[props(default)] required: bool) -> Element {
    let updates = use_context::<AndroidUpdates>();
    rsx! {
        if let Some(Ok(Some(update))) = &*updates.0.read() {
            button {
                class: "button button-secondary desktop-update-badge",
                onclick: {
                    let url = update.url.clone();
                    move |_| {
                        if let Err(error) = webbrowser::open(&url) {
                            log::warn!("Could not open update: {error}");
                        }
                    }
                },
                "Update to v{update.version} ↗"
            }
        } else if required {
            button {
                class: "button button-secondary",
                onclick: move |_| {
                    let _ = webbrowser::open(RELEASES_URL);
                },
                "Download latest ↗"
            }
        }
    }
}
