//! `/online` and `/join/:code` entry pages.
//!
//! Both are thin redirects: the nickname is generated automatically (see
//! `state::nickname`) and editable once the lobby has loaded.

use dioxus::prelude::*;
use tantrix_net::{generate_room_code, ROOM_CODE_LEN};

use crate::route::{Route, RoleParam};

/// `/online` landing page: host a fresh room or enter a code to join.
#[component]
pub fn OnlineEntry() -> Element {
    let nav = navigator();
    let code_input = use_signal(String::new);
    let error = use_signal(|| Option::<String>::None);

    rsx! {
        div { class: "title-screen",
            div { class: "title-content",
                div { class: "title-logo-area",
                    h1 { class: "title-logo", "ONLINE" }
                    p { class: "title-subtitle", "Play over WebRTC" }
                }

                div { class: "online-entry",
                    div { class: "online-actions",
                        button {
                            class: "title-btn title-btn-primary",
                            onclick: move |_| {
                                let _ = nav.push(Route::OnlineRoom {
                                    code: generate_room_code(),
                                    role: RoleParam::Host,
                                });
                            },
                            div { class: "title-btn-inner", "Host a new room" }
                        }

                        div { class: "online-join",
                            label { class: "online-field",
                                span { "Join code" }
                                input {
                                    r#type: "text",
                                    maxlength: "{ROOM_CODE_LEN}",
                                    placeholder: "ABCXYZ",
                                    value: "{code_input}",
                                    oninput: move |e| {
                                        let mut c = code_input;
                                        c.set(e.value().to_ascii_uppercase());
                                    },
                                    onkeydown: move |e| {
                                        if e.key() == Key::Enter {
                                            try_join(code_input, error, nav);
                                        }
                                    },
                                }
                            }
                            button {
                                class: "title-btn title-btn-primary",
                                onclick: move |_| try_join(code_input, error, nav),
                                div { class: "title-btn-inner", "Join room" }
                            }
                        }
                    }

                    if let Some(err) = error.read().clone() {
                        p { class: "online-error", "{err}" }
                    }

                    button {
                        class: "title-btn-link",
                        onclick: move |_| { let _ = nav.push(Route::MainMenu {}); },
                        "← Back to menu"
                    }
                }
            }
        }
    }
}

/// Deep-link `/join/ABCXYZ` — validates the code, then drops straight into
/// the lobby with an auto-generated nickname.
#[component]
pub fn JoinRoom(code: String) -> Element {
    let nav = navigator();
    let code_upper = code.trim().to_ascii_uppercase();

    let code_is_valid = code_upper.len() == ROOM_CODE_LEN
        && code_upper.bytes().all(|b| b.is_ascii_uppercase());

    // Auto-redirect into the room on first mount if the code looks sane.
    use_hook({
        let code = code_upper.clone();
        let valid = code_is_valid;
        move || {
            if valid {
                let _ = nav.push(Route::OnlineRoom {
                    code,
                    role: RoleParam::Client,
                });
            }
        }
    });

    rsx! {
        div { class: "title-screen",
            div { class: "title-content",
                div { class: "title-logo-area",
                    h1 { class: "title-logo", "JOIN ROOM" }
                    p { class: "title-subtitle", "Room code: {code_upper}" }
                }

                div { class: "online-entry",
                    if code_is_valid {
                        p { class: "lobby-waiting", "Connecting…" }
                    } else {
                        p { class: "online-error", "This link doesn't look like a valid room code." }
                        button {
                            class: "title-btn title-btn-primary",
                            onclick: move |_| { let _ = nav.push(Route::OnlineEntry {}); },
                            div { class: "title-btn-inner", "Back to online menu" }
                        }
                    }
                }
            }
        }
    }
}

fn try_join(
    code_input: Signal<String>,
    mut error: Signal<Option<String>>,
    nav: dioxus_router::Navigator,
) {
    let code = code_input.read().trim().to_ascii_uppercase();
    if code.len() != ROOM_CODE_LEN {
        error.set(Some("Room codes are 6 characters.".into()));
        return;
    }
    let _ = nav.push(Route::OnlineRoom {
        code,
        role: RoleParam::Client,
    });
}
