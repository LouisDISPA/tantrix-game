//! `/online` and `/join/:code` entry pages.

use dioxus::prelude::*;
use dioxus_router::Navigator;
use tantrix_net::{generate_room_code, ROOM_CODE_LEN};

use crate::route::Route;

/// Landing page: pick "Host" or "Join".
#[component]
pub fn OnlineEntry() -> Element {
    let nav = navigator();
    let nickname = use_signal(default_nickname);
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
                    label { class: "online-field",
                        span { "Nickname" }
                        input {
                            r#type: "text",
                            maxlength: "20",
                            value: "{nickname}",
                            oninput: move |e| { let mut n = nickname; n.set(e.value()); },
                        }
                    }

                    div { class: "online-actions",
                        button {
                            class: "title-btn title-btn-primary",
                            onclick: move |_| try_host(nickname, error, nav),
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
                                            try_join(nickname, code_input, error, nav);
                                        }
                                    },
                                }
                            }
                            button {
                                class: "title-btn title-btn-primary",
                                onclick: move |_| try_join(nickname, code_input, error, nav),
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

/// Deep-link `/join/ABCDEF` — collects nickname then redirects to the room.
#[component]
pub fn JoinRoom(code: String) -> Element {
    let nav = navigator();
    let nickname = use_signal(default_nickname);
    let error = use_signal(|| Option::<String>::None);
    let code_upper = code.trim().to_ascii_uppercase();

    let code_is_valid = code_upper.len() == ROOM_CODE_LEN
        && code_upper.bytes().all(|b| b.is_ascii_uppercase());
    let join_code = code_upper.clone();
    let join_code_for_enter = join_code.clone();
    let join_code_for_click = join_code.clone();

    rsx! {
        div { class: "title-screen",
            div { class: "title-content",
                div { class: "title-logo-area",
                    h1 { class: "title-logo", "JOIN ROOM" }
                    p { class: "title-subtitle", "Room code: {code_upper}" }
                }

                div { class: "online-entry",
                    if !code_is_valid {
                        p { class: "online-error", "This link doesn't look like a valid room code." }
                    } else {
                        label { class: "online-field",
                            span { "Nickname" }
                            input {
                                r#type: "text",
                                maxlength: "20",
                                value: "{nickname}",
                                autofocus: true,
                                oninput: move |e| { let mut n = nickname; n.set(e.value()); },
                                onkeydown: move |e| {
                                    if e.key() == Key::Enter {
                                        try_submit(nickname, error, nav, join_code_for_enter.clone());
                                    }
                                },
                            }
                        }
                        button {
                            class: "title-btn title-btn-primary",
                            onclick: move |_| try_submit(nickname, error, nav, join_code_for_click.clone()),
                            div { class: "title-btn-inner", "Join" }
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

fn try_host(
    nickname: Signal<String>,
    mut error: Signal<Option<String>>,
    nav: Navigator,
) {
    let nick = nickname.read().clone();
    if nick.trim().is_empty() {
        error.set(Some("Enter a nickname first.".into()));
        return;
    }
    let code = generate_room_code();
    let _ = nav.push(Route::OnlineRoom {
        code,
        nickname: nick,
        role: "host".into(),
    });
}

fn try_join(
    nickname: Signal<String>,
    code_input: Signal<String>,
    mut error: Signal<Option<String>>,
    nav: Navigator,
) {
    let nick = nickname.read().clone();
    if nick.trim().is_empty() {
        error.set(Some("Enter a nickname first.".into()));
        return;
    }
    let code = code_input.read().trim().to_ascii_uppercase();
    if code.len() != ROOM_CODE_LEN {
        error.set(Some("Room codes are 6 characters.".into()));
        return;
    }
    let _ = nav.push(Route::OnlineRoom {
        code,
        nickname: nick,
        role: "client".into(),
    });
}

fn try_submit(
    nickname: Signal<String>,
    mut error: Signal<Option<String>>,
    nav: Navigator,
    code: String,
) {
    let nick = nickname.read().clone();
    if nick.trim().is_empty() {
        error.set(Some("Enter a nickname first.".into()));
        return;
    }
    let _ = nav.push(Route::OnlineRoom {
        code,
        nickname: nick,
        role: "client".into(),
    });
}

fn default_nickname() -> String {
    "Player".to_string()
}
