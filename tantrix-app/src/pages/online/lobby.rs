//! Lobby: shows joined peers, lets each player claim a color, and lets the
//! host start the game.

use dioxus::prelude::*;
use tantrix_game::Color;
use tantrix_net::SessionCommand;
use tantrix_proto::LobbyState;

use crate::state::online_state::{OnlineCtx, OnlinePhase};

const COLORS: [Color; 4] = [Color::Red, Color::Blue, Color::Yellow, Color::Green];

#[component]
pub fn OnlineLobby() -> Element {
    // When navigated to directly, /online/lobby isn't used — the real entry
    // is /online/:code/:nickname/:role via `OnlineRoom`. This component
    // exists so the pages module can re-export it for the router tree.
    rsx! { OnlineLobbyView {} }
}

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

    rsx! {
        div { class: "title-screen",
            div { class: "title-content lobby-content",
                div { class: "lobby-header",
                    h1 { class: "title-logo", "LOBBY" }
                    div { class: "lobby-room-chip",
                        span { class: "lobby-room-code", "{room_code}" }
                        span { class: "lobby-share-url", "{share_url}" }
                    }
                }

                div { class: "lobby-peers",
                    h2 { "Players" }
                    PeerList { lobby: lobby.clone() }
                }

                div { class: "lobby-colors",
                    h2 { "Choose a color" }
                    div { class: "lobby-color-grid",
                        for color in COLORS.iter().copied() {
                            ColorSeat { color, lobby: lobby.clone(), my_color }
                        }
                    }
                    button {
                        class: "title-btn-link",
                        disabled: my_color.is_none(),
                        onclick: move |_| {
                            if let Some(s) = ctx.sender.read().as_ref() {
                                s.send(SessionCommand::PickColor(None));
                            }
                        },
                        "Release my seat"
                    }
                }

                if let Some(err) = error {
                    p { class: "online-error", "{err}" }
                }

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
                            format!("background: var(--color-{});", color_var(c))
                        } else {
                            String::from("background: var(--bg-muted);")
                        },
                    }
                    span { class: "lobby-peer-name", "{nickname}" }
                    if is_host {
                        span { class: "lobby-peer-tag", "host" }
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

    let label = seat.as_ref().map(|s| s.nickname.clone()).unwrap_or_default();
    let class = if is_mine {
        "lobby-color-chip mine"
    } else if taken_by_other {
        "lobby-color-chip taken"
    } else {
        "lobby-color-chip free"
    };

    rsx! {
        button {
            class: "{class}",
            disabled: taken_by_other,
            onclick: move |_| {
                if let Some(s) = ctx.sender.read().as_ref() {
                    s.send(SessionCommand::PickColor(Some(color)));
                }
            },
            style: format!("--seat-color: var(--color-{});", color_var(color)),
            span { class: "lobby-color-dot" }
            span { class: "lobby-color-label", "{color_name(color)}" }
            if let Some(name) = seat.as_ref().map(|s| s.nickname.clone()) {
                span { class: "lobby-color-player", "{name}" }
            } else {
                span { class: "lobby-color-player dim", "open" }
            }
            if is_mine {
                span { class: "lobby-color-tag", "you" }
            }
            // Silence unused var warnings when label is present but not used yet.
            { let _ = label; rsx! {} }
        }
    }
}

fn color_var(c: Color) -> &'static str {
    match c {
        Color::Red => "red",
        Color::Blue => "blue",
        Color::Yellow => "yellow",
        Color::Green => "green",
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

fn share_link(code: &str) -> String {
    // Hash-route share link; matchbox_server is untouched by this — the
    // fragment is client-side only. `window.location.origin` would give
    // the accurate base, but during initial dev we just hint the shape.
    format!("/#/join/{}", code)
}
