//! Click-to-copy invite chip. Self-contained: owns the "just copied"
//! flash state and drives the browser clipboard write itself. The parent
//! only supplies the URL.

use std::time::Duration;

use dioxus::prelude::*;

use crate::util::sleep;

#[component]
pub fn ShareChip(url: ReadSignal<String>) -> Element {
    let mut copied = use_signal(|| false);
    // Incremented on every click; the reset timer only clears `copied` if
    // it still matches the generation it captured, so rapid re-clicks
    // don't get their "Copied!" flipped back early by a stale timer.
    let mut generation = use_signal(|| 0u64);

    let on_click = move |_| {
        let text = url.read().clone();
        let my_gen = *generation.peek() + 1;
        generation.set(my_gen);
        spawn(async move {
            if copy_to_clipboard(text).await {
                copied.set(true);
                sleep(Duration::from_millis(1500)).await;
                if *generation.peek() == my_gen {
                    copied.set(false);
                }
            }
        });
    };

    rsx! {
        button {
            class: "button button-secondary lobby-share-chip",
            class: if copied() { "copied" },
            onclick: on_click,
            span { class: "lobby-share-label",
                if copied() {
                    "Copied!"
                } else {
                    "Click to copy invite link"
                }
            }
            span { class: "lobby-share-url truncate", "{url}" }
        }
    }
}

/// Copy `text` to the system clipboard. Returns `true` on confirmed
/// success so the caller only flashes "Copied!" when the write actually
/// landed (permission denials, insecure origins, and missing
/// `navigator.clipboard` all resolve to `false`). No-op off the web.
#[cfg(target_arch = "wasm32")]
async fn copy_to_clipboard(text: String) -> bool {
    let Some(win) = web_sys::window() else {
        return false;
    };
    let promise = win.navigator().clipboard().write_text(&text);
    wasm_bindgen_futures::JsFuture::from(promise).await.is_ok()
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "desktop")))]
async fn copy_to_clipboard(_text: String) -> bool {
    false
}

#[cfg(all(not(target_arch = "wasm32"), feature = "desktop"))]
async fn copy_to_clipboard(text: String) -> bool {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text))
        .is_ok()
}
