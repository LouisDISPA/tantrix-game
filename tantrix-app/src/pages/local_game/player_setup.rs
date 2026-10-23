use dioxus::prelude::*;

use crate::components::color_swatch::ColorSwatch;
use crate::components::difficulty_picker::DifficultyPicker;
use crate::components::menu_button::MenuButton;
use crate::components::menu_card::MenuCard;
use crate::components::menu_screen::MenuScreen;
use crate::route::Route;
use crate::state::config::{GameConfig, PlayerKind};
use crate::svg::paths::color_name as color_slug;

/// The new-game form: who plays each color. Start Game hands the chosen
/// config to `on_start`.
#[component]
pub fn PlayerSetup(seed: Option<u64>, on_start: EventHandler<GameConfig>) -> Element {
    let mut config = use_signal(|| GameConfig::new(seed));
    let nav = navigator();
    let can_start = use_memo(move || config.read().active_count() >= 2);

    rsx! {
        MenuScreen {
            MenuCard {
                title: "New Game",
                subtitle: "Set up players",
                back: move |_| {
                    let _ = nav.push(Route::MainMenu {});
                },
                div { class: "player-slots stack",
                    for (color, kind) in config.read().players {
                        div {
                            key: "{color:?}",
                            class: "surface surface-inset player-slot color-{color_slug(color)} row gap-3",
                            class: if kind != PlayerKind::Off { "active" },
                            class: if matches!(kind, PlayerKind::Ai(_)) { "has-level" },
                            ColorSwatch { color }
                            div { class: "player-label", "{color}" }
                            button {
                                class: match kind {
                                    PlayerKind::Off => "button button-small player-toggle off",
                                    PlayerKind::Human => "button button-small player-toggle human",
                                    PlayerKind::Ai(_) => "button button-small player-toggle ai",
                                },
                                onclick: move |_| config.write().cycle_player(color),
                                "{kind.label()}"
                            }
                            if let PlayerKind::Ai(level) = kind {
                                DifficultyPicker {
                                    level,
                                    on_change: move |difficulty| {
                                        config.write().set_difficulty(color, difficulty);
                                    },
                                }
                            }
                        }
                    }
                }
                MenuButton {
                    full_width: true,
                    disabled: !can_start(),
                    on_click: move |_| on_start.call(config.read().clone()),
                    "Start Game"
                }
            }
        }
    }
}
