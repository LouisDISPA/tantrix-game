//! `/online` and `/join/:code` entry pages.
//!
//! Both are thin redirects: the nickname is generated automatically (see
//! `state::nickname`) and editable once the lobby has loaded.

use dioxus::prelude::*;
use tantrix_net::{generate_room_code, normalize_room_code, ROOM_CODE_LEN};

use crate::components::menu_button::MenuButton;
use crate::components::menu_card::MenuCard;
use crate::components::menu_screen::MenuScreen;
use crate::route::Route;
use crate::state::room_connection;

/// `/online` landing page: create a fresh room or enter a code to join.
#[component]
pub fn OnlineEntry() -> Element {
    let nav = navigator();
    let code_input = use_signal(String::new);
    let error = use_signal(|| Option::<String>::None);
    let mut server_ready = use_signal(|| true);

    rsx! {
        MenuScreen {
            MenuCard {
                title: "Online",
                subtitle: "Play together online",
                back: move |_| {
                    let _ = nav.push(Route::MainMenu {});
                },
                div { class: "online-entry",
                    div { class: "online-actions",
                        MenuButton {
                            full_width: true,
                            disabled: !server_ready(),
                            on_click: move |_| {
                                if !server_ready() {
                                    return;
                                }
                                let code = generate_room_code();
                                room_connection::mark_create(&code);
                                let _ = nav.push(Route::OnlineRoom { code });
                            },
                            "Create a room"
                        }

                        div { class: "surface surface-inset online-join",
                            label { class: "field-group online-field",
                                span { class: "field-label", "Join code" }
                                input {
                                    class: "field",
                                    r#type: "text",
                                    maxlength: "{ROOM_CODE_LEN}",
                                    placeholder: "ABCXYZ",
                                    value: "{code_input}",
                                    oninput: move |e| {
                                        let mut c = code_input;
                                        c.set(e.value().to_ascii_uppercase());
                                    },
                                    onkeydown: move |e| {
                                        if e.key() == Key::Enter && server_ready() {
                                            try_join(code_input, error, nav);
                                        }
                                    },
                                }
                            }
                            MenuButton {
                                full_width: true,
                                disabled: !server_ready(),
                                on_click: move |_| {
                                    if server_ready() {
                                        try_join(code_input, error, nav);
                                    }
                                },
                                "Join room"
                            }
                        }
                    }

                    DesktopServerSettings { on_ready: move |ready| server_ready.set(ready) }

                    if let Some(err) = error.read().clone() {
                        p { class: "alert-banner online-error", {err} }
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
        MenuScreen {
            MenuCard { title: "Join Room", subtitle: "Room code",
                div { class: "online-entry",
                    p { class: "online-room-code", {code_upper} }
                    if code_is_valid {
                        p { class: "lobby-waiting", "Connecting…" }
                    } else {
                        p { class: "alert-banner online-error",
                            "This link doesn't look like a valid room code."
                        }
                        MenuButton {
                            full_width: true,
                            on_click: move |_| {
                                let _ = nav.push(Route::OnlineEntry {});
                            },
                            "Back to online menu"
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

/// Desktop-only settings; the browser continues to use its own origin.
#[component]
fn DesktopServerSettings(on_ready: EventHandler<bool>) -> Element {
    #[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
    {
        use crate::state::desktop_settings::{self, DEFAULT_SERVER};
        let mut address = use_signal(desktop_settings::server_address);
        let mut error = use_signal(|| None::<String>);
        let mut open = use_signal(|| false);
        let mut save_address = move |close: bool| {
            let input = address.read().clone();
            let result = desktop_settings::normalize_server(&input).and_then(|server| {
                desktop_settings::save("server-address", &server).map(|_| server)
            });
            match result {
                Ok(server) => {
                    address.set(server);
                    error.set(None);
                    on_ready.call(true);
                    if close {
                        open.set(false);
                    }
                }
                Err(message) => {
                    error.set(Some(message));
                    on_ready.call(false);
                }
            }
        };
        return rsx! {
            div {
                class: "connection-settings",
                class: if open() { "open" },
                onkeydown: move |event| {
                    if open() && event.key() == Key::Escape {
                        event.stop_propagation();
                        open.set(false);
                    }
                },
                if open() {
                    div {
                        class: "hud-menu-backdrop",
                        onclick: move |_| open.set(false),
                    }
                }
                button {
                    class: "button button-ghost button-small connection-settings-toggle",
                    aria_expanded: "{open()}",
                    aria_controls: "connection-settings-panel",
                    onclick: move |_| open.set(!open()),
                    "Connection settings"
                }
                div {
                    id: "connection-settings-panel",
                    class: "surface surface-raised surface-blur connection-settings-panel",
                    label { class: "field-group online-field",
                        span { class: "field-label", "Server address" }
                        input {
                            class: "field",
                            r#type: "text",
                            value: "{address}",
                            oninput: move |event| {
                                let input = event.value();
                                on_ready.call(desktop_settings::normalize_server(&input).is_ok());
                                address.set(input);
                                error.set(None);
                            },
                            onblur: move |_| save_address(false),
                        }
                    }
                    div { class: "connection-settings-actions",
                        button {
                            class: "button button-small button-secondary",
                            onclick: move |_| {
                                match desktop_settings::save("server-address", DEFAULT_SERVER) {
                                    Ok(()) => {
                                        address.set(DEFAULT_SERVER.into());
                                        error.set(None);
                                        on_ready.call(true);
                                    }
                                    Err(message) => error.set(Some(message)),
                                }
                            },
                            "Reset to default"
                        }
                        button {
                            class: "button button-small button-primary",
                            onclick: move |_| save_address(true),
                            "Validate"
                        }
                    }
                    if let Some(message) = error.read().as_ref() {
                        p { class: "alert-banner online-error", {message.clone()} }
                    }
                }
            }
        };
    }
    #[cfg(not(all(feature = "desktop", not(target_arch = "wasm32"))))]
    rsx! {}
}
