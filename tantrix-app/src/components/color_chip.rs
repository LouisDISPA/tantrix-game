//! Hex "pick this color" button used in the lobby. Pure presentation:
//! the parent decides which state this chip is in.

use dioxus::prelude::*;
use tantrix_game::Color;

use crate::components::color_swatch::ColorSwatch;
use crate::svg::paths::color_name as color_slug;

/// Mutually-exclusive visual states for a color chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChipState {
    /// Nobody has claimed this color.
    Free,
    /// The local viewer owns this seat.
    Mine,
    /// Another peer owns this seat (it can still be taken over).
    TakenByOther,
    /// Held by a player who is offline; anyone may take it over.
    Offline,
}

#[component]
pub fn ColorChip(
    color: Color,
    state: ColorChipState,
    seat_label: ReadSignal<Option<String>>,
    on_click: EventHandler<()>,
) -> Element {
    let state_class = match state {
        ColorChipState::Mine => "mine",
        ColorChipState::TakenByOther => "taken",
        ColorChipState::Free => "free",
        ColorChipState::Offline => "offline",
    };
    let is_mine = state == ColorChipState::Mine;
    let taken = state == ColorChipState::TakenByOther;
    let color_class = format!("color-{}", color_slug(color));

    rsx! {
        button {
            class: "lobby-color-chip {state_class} {color_class}",
            onclick: move |_| on_click.call(()),
            ColorSwatch { color, large: true, selected: is_mine }
            div { class: "lobby-color-meta",
                span { class: "lobby-color-label", "{color}" }
                if let Some(name) = seat_label().clone() {
                    span { class: "lobby-color-player", "{name}" }
                } else {
                    span { class: "lobby-color-player empty", "Open" }
                }
            }
            if is_mine {
                span { class: "lobby-color-tag", "You" }
            } else if taken {
                span { class: "lobby-color-tag dim", "Taken" }
            } else if state == ColorChipState::Offline {
                span { class: "lobby-color-tag dim", "Offline" }
            }
        }
    }
}
