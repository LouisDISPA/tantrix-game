//! Lobby: shows joined peers, lets each player claim a color, and lets the
//! host start the game.

use dioxus::prelude::*;
use tantrix_game::Color;
use tantrix_net::SessionCommand;
use tantrix_proto::LobbyState;

use crate::state::online_state::{OnlineCtx, OnlinePhase};

const COLORS: [Color; 4] = [Color::Red, Color::Blue, Color::Yellow, Color::Green];

#[component]
pub fn OnlineLobbyView() -> Element {
    let ctx = use_context::<OnlineCtx>();
    let state = ctx.state.read();

    let OnlinePhase::Lobby { lobby } = &state.phase else {
        return rsx! {};
    };
    let lobby = lobby.clone();

    let is_host = state.is_host();
    let my_color = state.my_color();
    let error = state.last_error.clone();
    let room_code = state.room_code.clone();
    let seated_count = lobby.seats.values().filter(|s| s.is_some()).count();

    drop(state);

    let share_url = share_link(&room_code);
    let copied = use_signal(|| false);

    rsx! {
        div { class: "title-screen lobby-screen",
            div { class: "title-content lobby-content",
                div { class: "lobby-header",
                    h1 { class: "title-logo", "LOBBY" }
                    div { class: "lobby-room-code", "{room_code}" }
                    ShareUrlChip { url: share_url, copied }
                }

                section { class: "lobby-section",
                    h2 { class: "lobby-section-title", "Players" }
                    PeerList { lobby: lobby.clone() }
                }

                section { class: "lobby-section",
                    h2 { class: "lobby-section-title", "Pick your color" }
                    div { class: "lobby-color-grid",
                        for color in COLORS.iter().copied() {
                            ColorSeat { color, lobby: lobby.clone(), my_color }
                        }
                    }
                    if my_color.is_some() {
                        button {
                            class: "lobby-release-btn",
                            onclick: move |_| {
                                if let Some(s) = ctx.sender.read().as_ref() {
                                    s.send(SessionCommand::PickColor(None));
                                }
                            },
                            "Release my seat"
                        }
                    }
                }

                if let Some(err) = error {
                    p { class: "lobby-error", "{err}" }
                }

                div { class: "lobby-actions",
                    if is_host {
                        button {
                            class: "title-btn title-btn-primary",
                            disabled: seated_count < 2,
                            onclick: move |_| {
                                if let Some(s) = ctx.sender.read().as_ref() {
                                    s.send(SessionCommand::StartGame);
                                }
                            },
                            div { class: "title-btn-inner",
                                if seated_count < 2 {
                                    "Waiting for players…"
                                } else {
                                    "Start game"
                                }
                            }
                        }
                    } else {
                        p { class: "lobby-waiting", "Waiting for the host to start the game…" }
                    }
                }
            }
        }
    }
}

#[component]
fn ShareUrlChip(url: String, copied: Signal<bool>) -> Element {
    let url_for_copy = url.clone();

    rsx! {
        button {
            class: "lobby-share-chip",
            class: if copied() { "copied" },
            onclick: move |_| {
                copy_to_clipboard(url_for_copy.clone());
                let mut c = copied;
                c.set(true);
                spawn(async move {
                    gloo_sleep(1500).await;
                    c.set(false);
                });
            },
            span { class: "lobby-share-label",
                if copied() { "Copied!" } else { "Click to copy invite link" }
            }
            span { class: "lobby-share-url", "{url}" }
        }
    }
}

#[component]
fn PeerList(lobby: LobbyState) -> Element {
    let mut rows: Vec<(String, Option<Color>, bool)> = Vec::new();

    for (color, slot) in lobby.seats.iter() {
        if let Some(seat) = slot {
            let is_host = seat.peer_id == lobby.host;
            rows.push((seat.nickname.clone(), Some(color), is_host));
        }
    }
    for seat in lobby.unseated.iter() {
        let is_host = seat.peer_id == lobby.host;
        rows.push((seat.nickname.clone(), None, is_host));
    }

    rsx! {
        ul { class: "lobby-peer-list",
            for (nickname, color, is_host) in rows {
                li { class: "lobby-peer-row",
                    span { class: "lobby-peer-dot",
                        style: if let Some(c) = color {
                            format!("background: var(--color-{});", color_slug(c))
                        } else {
                            String::from("background: var(--border-color);")
                        },
                    }
                    span { class: "lobby-peer-name", "{nickname}" }
                    if is_host {
                        span { class: "lobby-peer-tag host", "host" }
                    }
                    if color.is_none() {
                        span { class: "lobby-peer-tag dim", "picking…" }
                    }
                }
            }
        }
    }
}

#[component]
fn ColorSeat(color: Color, lobby: LobbyState, my_color: Option<Color>) -> Element {
    let ctx = use_context::<OnlineCtx>();
    let seat = lobby.seats[color].clone();
    let taken_by_other = matches!(&seat, Some(_)) && my_color != Some(color);
    let is_mine = my_color == Some(color);

    let class = if is_mine {
        "lobby-color-chip mine"
    } else if taken_by_other {
        "lobby-color-chip taken"
    } else {
        "lobby-color-chip free"
    };

    let slug = color_slug(color);

    rsx! {
        button {
            class: "{class}",
            disabled: taken_by_other,
            onclick: move |_| {
                if let Some(s) = ctx.sender.read().as_ref() {
                    s.send(SessionCommand::PickColor(Some(color)));
                }
            },
            style: "--seat-color: var(--color-{slug});",
            div { class: "lobby-color-hex",
                div { class: "lobby-color-hex-inner" }
                if is_mine {
                    div { class: "lobby-color-check", "✓" }
                }
            }
            div { class: "lobby-color-meta",
                span { class: "lobby-color-label", "{color_name(color)}" }
                if let Some(name) = seat.as_ref().map(|s| s.nickname.clone()) {
                    span { class: "lobby-color-player", "{name}" }
                } else {
                    span { class: "lobby-color-player empty", "Open" }
                }
            }
            if is_mine {
                span { class: "lobby-color-tag", "You" }
            } else if taken_by_other {
                span { class: "lobby-color-tag dim", "Taken" }
            }
        }
    }
}

fn color_name(c: Color) -> &'static str {
    match c {
        Color::Red => "Red",
        Color::Blue => "Blue",
        Color::Yellow => "Yellow",
        Color::Green => "Green",
    }
}

fn color_slug(c: Color) -> &'static str {
    match c {
        Color::Red => "red",
        Color::Blue => "blue",
        Color::Yellow => "yellow",
        Color::Green => "green",
    }
}

/// Build a shareable URL of the shape `<origin>/#/join/CODE`. Falls back to
/// a relative fragment if `window` isn't available (e.g. SSR or native).
fn share_link(code: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            if let Ok(origin) = win.location().origin() {
                return format!("{origin}/#/join/{code}");
            }
        }
    }
    format!("/#/join/{code}")
}

/// Copy `text` to the system clipboard using the browser's async clipboard
/// API. Silently no-ops off the web.
fn copy_to_clipboard(text: String) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            let clipboard = win.navigator().clipboard();
            let _ = clipboard.write_text(&text);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = text;
}

async fn gloo_sleep(ms: u32) {
    #[cfg(feature = "web")]
    gloo_timers::future::TimeoutFuture::new(ms).await;
    #[cfg(not(feature = "web"))]
    let _ = ms;
}
