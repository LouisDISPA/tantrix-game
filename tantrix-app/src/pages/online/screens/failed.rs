//! Terminal screen shown when the session ends — either because the host
//! rejected the connection or the transport died.

use dioxus::prelude::*;
use tantrix_proto::RejectCode;

use crate::state::online_state::FailureReason;

#[component]
pub fn FailedView(reason: FailureReason) -> Element {
    let nav = navigator();
    let (title, is_rejection) = heading_for(&reason);
    let detail = reason.display();
    rsx! {
        div { class: "title-screen",
            div { class: "title-content",
                h1 { class: "title-logo", "{title}" }
                p { class: "title-subtitle", "{detail}" }
                button {
                    class: "title-btn title-btn-primary",
                    onclick: move |_| {
                        let _ = nav.push(crate::route::Route::OnlineEntry {});
                    },
                    div { class: "title-btn-inner",
                        if is_rejection { "Back to online menu" } else { "Back to menu" }
                    }
                }
            }
        }
    }
}

/// A rejection message from the host is user-actionable; a generic
/// transport failure is not. Tailor the heading so the former stands out as
/// "room full / kicked" rather than "something broke".
fn heading_for(reason: &FailureReason) -> (&'static str, bool) {
    match reason {
        FailureReason::Rejected(RejectCode::Kicked) => ("Removed from room", true),
        FailureReason::Rejected(_) => ("Can't join this room", true),
        FailureReason::Transport(_) => ("Disconnected", false),
    }
}
