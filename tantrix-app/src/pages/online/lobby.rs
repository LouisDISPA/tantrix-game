//! Lobby: shows joined peers, lets each player claim a color and edit
//! their nickname, and lets the host start the game.

use std::time::Duration;

use dioxus::prelude::*;
use tantrix_game::Color;
use tantrix_net::SessionCommand;
use tantrix_proto::{LobbyState, PeerId};

use crate::state::nickname;
use crate::state::online_state::{OnlineCtx, OnlinePhase};
use crate::svg::paths::color_name as color_slug;
use crate::util::sleep;

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
    let my_peer_id = state.my_peer_id.clone();
    let error = state.last_error.clone();
    let room_code = state.room_code.clone();
    let seated_count = lobby.seats.values().filter(|s| s.is_some()).count();
    let can_start = seated_count >= 2;

    drop(state);

    let share_url = share_link(&room_code);
    let copied = use_signal(|| false);

    let authoritative_nickname = my_peer_id
        .as_ref()
        .and_then(|id| find_my_nickname(&lobby, id))
        .unwrap_or_default();

    rsx! {
        div { class: "title-screen lobby-screen",
            div { class: "title-content lobby-content",
                div { class: "lobby-header",
                    h1 { class: "title-logo", "LOBBY" }
                    div { class: "lobby-room-code", "{room_code}" }
                    ShareUrlChip { url: share_url, copied }
                }

                section { class: "lobby-section",
                    h2 { class: "lobby-section-title", "Your nickname" }
                    NicknameEditor { authoritative: authoritative_nickname }
                }

                section { class: "lobby-section",
                    h2 { class: "lobby-section-title", "Players" }
                    PeerList { lobby: lobby.clone() }
                }

                section { class: "lobby-section",
                    h2 { class: "lobby-section-title", "Pick your color" }
                    div { class: "lobby-color-row",
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
                            disabled: !can_start,
                            onclick: move |_| {
                                if can_start {
                                    if let Some(s) = ctx.sender.read().as_ref() {
                                        s.send(SessionCommand::StartGame);
                                    }
                                }
                            },
                            div { class: "title-btn-inner",
                                if can_start { "Start game" } else { "Waiting for players…" }
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

/// Editable nickname field. `authoritative` is the name the host currently
/// has on record; we sync the draft to it on mount so auto-generated names
/// show up. The draft is committed on blur / Enter so we don't flood the
/// host with UpdateNickname messages on every keystroke.
#[component]
fn NicknameEditor(authoritative: String) -> Element {
    let ctx = use_context::<OnlineCtx>();
    let draft = use_signal(|| authoritative.clone());
    let initial = authoritative.clone();

    // Seed the draft the first time we see a non-empty authoritative name.
    use_hook(move || {
        let mut d = draft;
        if d.peek().is_empty() && !initial.is_empty() {
            d.set(initial);
        }
    });

    rsx! {
        input {
            class: "lobby-nickname-input",
            r#type: "text",
            maxlength: "24",
            value: "{draft}",
            oninput: move |e| { let mut d = draft; d.set(e.value()); },
            onblur: move |_| commit_nickname(ctx, draft),
            onkeydown: move |e| {
                if e.key() == Key::Enter {
                    commit_nickname(ctx, draft);
                }
            },
        }
    }
}

fn commit_nickname(ctx: OnlineCtx, draft: Signal<String>) {
    let trimmed = draft.read().trim().to_string();
    if trimmed.is_empty() {
        return;
    }
    if let Some(s) = ctx.sender.read().as_ref() {
        s.send(SessionCommand::UpdateNickname(trimmed.clone()));
    }
    nickname::persist(&trimmed);
}

fn find_my_nickname(lobby: &LobbyState, me: &PeerId) -> Option<String> {
    for slot in lobby.seats.values() {
        if let Some(seat) = slot {
            if &seat.peer_id == me {
                return Some(seat.nickname.clone());
            }
        }
    }
    lobby
        .unseated
        .iter()
        .find(|s| &s.peer_id == me)
        .map(|s| s.nickname.clone())
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
                    sleep(Duration::from_millis(1500)).await;
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
                        style: match color {
                            Some(c) => format!("background: var(--color-{});", color_slug(c)),
                            None => "background: var(--border-color);".to_string(),
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
                span { class: "lobby-color-label", "{color}" }
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
