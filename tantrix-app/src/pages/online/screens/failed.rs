//! Terminal screen shown when the session ends — either because the server
//! rejected the connection, the transport died, or connecting timed out.

use dioxus::prelude::*;

use crate::state::online_state::FailureReason;

#[component]
pub fn FailedView(reason: FailureReason, on_retry: EventHandler<()>) -> Element {
    let nav = navigator();
    let (title, is_rejection) = heading_for(&reason);
    let detail = reason.display();
    let retryable = reason.is_retryable();
    rsx! {
        div { class: "title-screen",
            div { class: "title-content",
                h1 { class: "title-logo", "{title}" }
                p { class: "title-subtitle", "{detail}" }
                if retryable {
                    button {
                        class: "title-btn title-btn-primary",
                        onclick: move |_| on_retry.call(()),
                        div { class: "title-btn-inner", "Retry" }
                    }
                }
                button {
                    class: if retryable { "title-btn-link" } else { "title-btn title-btn-primary" },
                    onclick: move |_| {
                        let _ = nav.push(crate::route::Route::OnlineEntry {});
                    },
                    if retryable {
                        "Back to online menu"
                    } else {
                        div { class: "title-btn-inner",
                            if is_rejection { "Back to online menu" } else { "Back to menu" }
                        }
                    }
                }
            }
        }
    }
}

/// A rejection message from the server is user-actionable; a generic
/// transport failure is not.
fn heading_for(reason: &FailureReason) -> (&'static str, bool) {
    match reason {
        FailureReason::Rejected(_) => ("Can't join this room", true),
        FailureReason::Transport(_) => ("Disconnected", false),
        FailureReason::Timeout => ("Couldn't connect", false),
    }
}
