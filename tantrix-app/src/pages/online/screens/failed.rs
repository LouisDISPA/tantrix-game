//! Terminal screen shown when the session ends — either because the server
//! rejected the connection, the transport died, or connecting timed out.

use dioxus::prelude::*;

use crate::components::menu_button::MenuButton;
use crate::components::menu_card::MenuCard;
use crate::components::menu_screen::MenuScreen;
use crate::state::online_state::FailureReason;

#[component]
pub fn FailedView(reason: FailureReason, on_retry: EventHandler<()>, server: String) -> Element {
    let nav = navigator();
    let server = server
        .split("/room/")
        .next()
        .unwrap_or(&server)
        .replacen("ws", "http", 1);
    let title = heading_for(&reason);
    let detail = reason.display();
    let retryable = reason.is_retryable();
    rsx! {
        MenuScreen {
            MenuCard { title, subtitle: detail,
                if cfg!(feature = "desktop") {
                    p { "Server: {server}" }
                }
                if retryable {
                    MenuButton {
                        full_width: true,
                        on_click: move |_| on_retry.call(()),
                        "Retry"
                    }
                }
                if retryable {
                    button {
                        class: "title-btn-link",
                        onclick: move |_| {
                            let _ = nav
                                .push(crate::route::Route::OnlineEntry {
                                });
                        },
                        "Back to online menu"
                    }
                } else {
                    MenuButton {
                        full_width: true,
                        on_click: move |_| {
                            let _ = nav
                                .push(crate::route::Route::OnlineEntry {
                                });
                        },
                        "Back to online menu"
                    }
                }
            }
        }
    }
}

fn heading_for(reason: &FailureReason) -> &'static str {
    match reason {
        FailureReason::Rejected(_) => "Can't join this room",
        FailureReason::Transport(_) => "Disconnected",
        FailureReason::Timeout => "Couldn't connect",
    }
}
