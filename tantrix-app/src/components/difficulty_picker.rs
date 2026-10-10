//! The difficulty levels of an AI seat as a row of chips. Pure presentation:
//! the parent owns the level that is set.

use dioxus::prelude::*;

use crate::state::config::Difficulty;

#[component]
pub fn DifficultyPicker(level: Difficulty, on_change: EventHandler<Difficulty>) -> Element {
    rsx! {
        div { class: "segmented-control difficulty-picker", role: "group", aria_label: "AI difficulty",
            for option in Difficulty::ALL {
                button {
                    key: "{option.label()}",
                    class: "button button-small button-ghost segment difficulty-chip",
                    class: if option == level { "on" },
                    aria_pressed: "{option == level}",
                    onclick: move |_| on_change.call(option),
                    "{option.label()}"
                }
            }
        }
    }
}
