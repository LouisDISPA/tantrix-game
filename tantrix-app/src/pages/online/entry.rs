//! `/online` and `/join/:code` entry pages.
//!
//! Both are thin redirects: the nickname is generated automatically (see
//! `state::nickname`) and editable once the lobby has loaded.

use dioxus::prelude::*;
use tantrix_net::{generate_room_code, normalize_room_code, ROOM_CODE_LEN};

use crate::route::Route;
use crate::state::room_connection;

/// `/online` landing page: create a fresh room or enter a code to join.
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
                    p { class: "title-subtitle", "Play together online" }
                }

                div { class: "online-entry",
                    div { class: "online-actions",
                        button {
                            class: "title-btn title-btn-primary",
                            onclick: move |_| {
                                let code = generate_room_code();
                                room_connection::mark_create(&code);
                                let _ = nav.push(Route::OnlineRoom {
                                    code,
                                });
                            },
                            div { class: "title-btn-inner", "Create a room" }
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
    let normalized_code = normalize_room_code(&code_upper);
    let code_is_valid = normalized_code.is_some();

    // Auto-redirect into the room if the code looks sane. An effect runs
    // after hydration (never during SSR): navigating mid-hydration would
    // mount the room twice and drop its first WebSocket. `replace` keeps
    // Back from bouncing through this redirect again.
    use_effect({
        let code = normalized_code.clone().unwrap_or_default();
        let valid = code_is_valid;
        move || {
            if valid {
                nav.replace(Route::OnlineRoom { code: code.clone() });
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
    let raw = code_input.read().clone();
    let Some(code) = normalize_room_code(&raw) else {
        error.set(Some(
            "Room codes use 6 letters and exclude I, O, and Q.".into(),
        ));
        return;
    };
    let _ = nav.push(Route::OnlineRoom { code });
}
