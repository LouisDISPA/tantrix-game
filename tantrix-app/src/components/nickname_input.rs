//! Editable nickname field. Owns the draft signal and the "resync draft
//! when the authoritative name first arrives" effect — auto-generated
//! names assigned by the server after mount show up automatically. Once the
//! user has typed anything, their input is never overwritten.
//!
//! The parent only feeds in the authoritative value and a commit callback.

use dioxus::prelude::*;

#[component]
pub fn NicknameInput(
    authoritative: ReadSignal<String>,
    max_len: u32,
    on_commit: EventHandler<String>,
) -> Element {
    let mut draft = use_signal(|| authoritative.read().clone());

    // Seed the draft the first time we see a non-empty authoritative
    // name. Re-runs when the server-assigned name changes but we stop
    // clobbering once the user has typed.
    use_effect(move || {
        let current = authoritative.read().clone();
        if draft.peek().is_empty() && !current.is_empty() {
            draft.set(current);
        }
    });

    rsx! {
        input {
            class: "lobby-nickname-input",
            r#type: "text",
            maxlength: "{max_len}",
            value: "{draft}",
            oninput: move |e| draft.set(e.value()),
            onblur: move |_| on_commit.call(draft.read().clone()),
            onkeydown: move |e| {
                if e.key() == Key::Enter {
                    on_commit.call(draft.read().clone());
                }
            },
        }
    }
}
