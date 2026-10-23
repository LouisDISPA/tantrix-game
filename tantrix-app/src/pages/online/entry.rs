//! `/online` and `/join/:code` entry pages.
//!
//! The nickname is generated automatically (see
//! `state::nickname`) and editable once the lobby has loaded.

use dioxus::prelude::*;
use tantrix_net::{normalize_room_code, ROOM_CODE_LEN};

use crate::components::menu_button::MenuButton;
use crate::components::menu_card::MenuCard;
use crate::components::menu_screen::MenuScreen;
use crate::route::Route;
#[cfg(not(any(feature = "desktop", feature = "android")))]
use crate::state::online_compatibility::DOWNLOAD_URL;
use crate::state::online_compatibility::{self, Compatibility};
use crate::state::{room_api, server_endpoints};

#[derive(Clone, PartialEq)]
enum EntryOperation {
    Create,
    Join(String),
}

/// `/online` landing page: create a fresh room or enter a code to join.
#[component]
pub fn OnlineEntry() -> Element {
    let nav = navigator();
    let code_input = use_signal(String::new);
    let mut operation = use_signal(|| None::<EntryOperation>);
    let mut error = use_signal(|| Option::<String>::None);
    let mut server_ready = use_signal(|| true);
    let mut server = use_signal(server_endpoints::server_origin);
    let mut mounted = use_signal(|| false);
    let mut compatibility = use_resource(move || async move {
        let server = server();
        if !mounted() {
            return None;
        }
        let result = online_compatibility::check(&server).await;
        Some((server, result))
    });
    use_effect(move || {
        let Some(op) = operation() else {
            return;
        };
        let origin = server.peek().clone();
        spawn(async move {
            let result = match op {
                EntryOperation::Create => room_api::create(&origin).await,
                EntryOperation::Join(code) => room_api::resolve(&origin, &code).await,
            };
            if *server.peek() != origin {
                error.set(Some(
                    "The server changed. Try again on the selected server.".into(),
                ));
                operation.set(None);
                return;
            }
            match result {
                Ok(room) => {
                    nav.push(Route::OnlineRoom { id: room.id });
                }
                Err(message) => {
                    error.set(Some(message));
                    operation.set(None);
                }
            }
        });
    });
    let can_join = use_memo(move || {
        operation.read().is_none()
            && server_ready()
            && (compatibility.state()() != UseResourceState::Ready
                || !matches!(
                    &*compatibility.read(),
                    Some(Some((checked, result)))
                        if *checked == server() && !matches!(result, Ok(Compatibility::Compatible))
                ))
    });

    rsx! {
        MenuScreen {
            MenuCard {
                title: "Online",
                subtitle: "Play together online",
                back: move |_| {
                    let _ = nav.push(Route::MainMenu {});
                },
                div {
                    class: "online-entry stack",
                    onmounted: move |_| mounted.set(true),
                    div { class: "online-actions stack gap-4",
                        MenuButton {
                            full_width: true,
                            disabled: !can_join(),
                            on_click: move |_| {
                                if !can_join() {
                                    return;
                                }
                                error.set(None);
                                operation.set(Some(EntryOperation::Create));
                            },
                            if matches!(&*operation.read(), Some(EntryOperation::Create)) { "Creating room…" } else { "Create a room" }
                        }

                        div { class: "surface surface-inset online-join stack",
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
                                        if e.key() == Key::Named(NamedKey::Enter) && can_join() {
                                            try_join(code_input, error, operation);
                                        }
                                    },
                                }
                            }
                            MenuButton {
                                full_width: true,
                                disabled: !can_join(),
                                on_click: move |_| {
                                    if can_join() {
                                        try_join(code_input, error, operation);
                                    }
                                },
                                "Join room"
                            }
                        }
                    }

                    ServerSettings {
                        on_ready: move |ready| server_ready.set(ready),
                        on_server_changed: move |origin| {
                            if *server.peek() != origin {
                                server.set(origin);
                            }
                        },
                    }

                    if compatibility.state()() == UseResourceState::Ready
                        && matches!(
                            &*compatibility.read(),
                            Some(Some((checked, _)))
                            if *checked == server()
                        )
                    {
                        match &*compatibility.read() {
                            Some(Some((_, Ok(Compatibility::Compatible)))) => rsx! {},
                            Some(Some((_, Ok(status)))) => rsx! {
                                div {
                                    class: "surface surface-inset online-compatibility stack gap-3",
                                    role: "alert",
                                    p {
                                        if !cfg!(any(feature = "desktop", feature = "android")) {
                                            "This page is out of date. Reload to play online."
                                        } else if *status == Compatibility::AppOutdated {
                                            "Update Tantrix to play online with this server."
                                        } else {
                                            "This server needs updating to work with your app."
                                        }
                                    }
                                    if !cfg!(any(feature = "desktop", feature = "android")) {
                                        MenuButton {
                                            full_width: true,
                                            on_click: move |_| {
                                                #[cfg(all(target_arch = "wasm32", feature = "web"))]
                                                if let Some(window) = web_sys::window() {
                                                    let _ = window.location().reload();
                                                }
                                            },
                                            "Reload"
                                        }
                                    } else {
                                        UpdateAction {}
                                    }
                                }
                            },
                            _ => rsx! {
                                p { class: "surface surface-raised surface-blur banner alert-banner online-error", role: "alert", "Couldn’t check server compatibility." }
                                button {
                                    class: "button button-secondary",
                                    onclick: move |_| compatibility.restart(),
                                    "Retry"
                                }
                            },
                        }
                    }

                    if let Some(err) = error.read().clone() {
                        p { class: "surface surface-raised surface-blur banner alert-banner online-error", role: "alert", {err} }
                    }
                }
            }
        }
    }
}

#[component]
fn UpdateAction() -> Element {
    #[cfg(feature = "desktop")]
    return rsx! {
        crate::pages::desktop_update::DesktopUpdate { required: true }
    };
    #[cfg(not(any(feature = "desktop", feature = "android")))]
    rsx! {
        a { class: "button button-primary button-full", href: DOWNLOAD_URL, "Download Tantrix ↗" }
    }
    #[cfg(feature = "android")]
    rsx! {
        crate::pages::android_update::AndroidUpdate { required: true }
    }
}

/// Deep-link `/join/ABCXYZ` — validates the code, then drops straight into
/// the lobby with an auto-generated nickname.
#[component]
pub fn JoinRoom(code: String) -> Element {
    let nav = navigator();
    let mut mounted = use_signal(|| false);
    let server = use_hook(server_endpoints::server_origin);
    let result = use_resource(move || {
        let server = server.clone();
        let code = code.clone();
        async move {
            if !mounted() {
                return None;
            }
            Some(match normalize_room_code(&code) {
                Some(code) => room_api::resolve(&server, &code).await,
                None => Err("Room codes use 6 letters and exclude I, O, and Q.".into()),
            })
        }
    });
    use_effect(move || {
        if let Some(Some(Ok(room))) = &*result.read() {
            nav.replace(Route::OnlineRoom {
                id: room.id.clone(),
            });
        }
    });
    rsx! {
        MenuScreen {
            MenuCard { title: "Join Room", subtitle: "Room invitation",
                div { class: "online-entry stack", onmounted: move |_| mounted.set(true),
                    if let Some(Some(Err(message))) = &*result.read() {
                        p { role: "alert", "{message}" }
                        MenuButton { full_width: true, on_click: move |_| { nav.push(Route::OnlineEntry {}); }, "Back to online menu" }
                    } else { p { role: "status", "Connecting…" } }
                }
            }
        }
    }
}

fn try_join(
    code_input: Signal<String>,
    mut error: Signal<Option<String>>,
    mut operation: Signal<Option<EntryOperation>>,
) {
    if operation.peek().is_some() {
        return;
    }
    let Some(code) = normalize_room_code(&code_input.read()) else {
        error.set(Some(
            "Room codes use 6 letters and exclude I, O, and Q.".into(),
        ));
        return;
    };
    error.set(None);
    operation.set(Some(EntryOperation::Join(code)));
}

/// Shared native settings; the browser continues to use its own origin.
#[component]
fn ServerSettings(
    on_ready: EventHandler<bool>,
    on_server_changed: EventHandler<String>,
) -> Element {
    #[cfg(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    ))]
    {
        use crate::state::server_settings::{self, DEFAULT_SERVER};
        let mut address = use_signal(server_settings::server_address);
        let mut error = use_signal(|| None::<String>);
        let mut open = use_signal(|| false);
        let mut save_address = move |close: bool| {
            let input = address.read().clone();
            let result = server_settings::save_server_address(&input);
            match result {
                Ok(server) => {
                    on_server_changed.call(server.clone());
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
                    if open() && event.key() == Key::Named(NamedKey::Escape) {
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
                            r#type: "url",
                            inputmode: "url",
                            autocapitalize: "none",
                            autocomplete: "url",
                            spellcheck: false,
                            value: "{address}",
                            oninput: move |event| {
                                let input = event.value();
                                on_ready.call(false);
                                address.set(input);
                                error.set(None);
                            },
                            onblur: move |_| save_address(false),
                        }
                    }
                    div { class: "connection-settings-actions row justify-between gap-3",
                        button {
                            class: "button button-small button-secondary",
                            onclick: move |_| {
                                match server_settings::save_server_address(DEFAULT_SERVER) {
                                    Ok(server) => {
                                        address.set(server.clone());
                                        on_server_changed.call(server);
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
                        p { class: "surface surface-raised surface-blur banner alert-banner online-error", {message.clone()} }
                    }
                }
            }
        };
    }
    #[cfg(not(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    )))]
    rsx! {}
}
