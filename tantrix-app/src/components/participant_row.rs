//! Presentational participant row. Pure: the parent packs display data
//! into `row` and supplies an optional kick callback.

use dioxus::prelude::*;

/// Color dot to the left of the nickname. Strings come from the color
/// slug (`"red"` / `"blue"` / …) or `"unseated"` when no color is claimed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatDot(pub &'static str);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantRowData {
    pub name: String,
    pub dot: SeatDot,
    pub role_tag: Option<&'static str>,
    pub is_you: bool,
    pub offline: bool,
}

#[component]
pub fn ParticipantRow(
    row: ParticipantRowData,
    on_kick: Option<EventHandler<()>>,
) -> Element {
    let dot = row.dot.0;
    let tag = row.role_tag;
    let tag_class = if tag == Some("host") { "host" } else { "dim" };

    rsx! {
        li {
            class: "lobby-peer-row",
            class: if row.offline { "offline" },
            span { class: "lobby-peer-dot {dot}" }
            span {
                class: "lobby-conn-dot",
                class: if !row.offline { "connected" } else { "disconnected" },
                aria_label: if !row.offline { "connected" } else { "disconnected" },
            }
            span { class: "lobby-peer-name", "{row.name}" }
            if row.is_you {
                span { class: "lobby-peer-tag dim", "you" }
            }
            if let Some(tag) = tag {
                span {
                    class: "lobby-peer-tag",
                    class: "{tag_class}",
                    "{tag}"
                }
            }
            if row.offline {
                span { class: "lobby-peer-tag dim", "offline" }
            }
            if let Some(kick) = on_kick {
                button {
                    class: "lobby-kick-btn",
                    onclick: move |_| kick.call(()),
                    "Kick"
                }
            }
        }
    }
}
