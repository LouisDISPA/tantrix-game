use dioxus::prelude::*;

use crate::components::color_swatch::ColorSwatch;
use crate::components::menu_button::MenuButton;
use crate::components::menu_card::MenuCard;
use crate::components::menu_screen::MenuScreen;
use crate::route::Route;
use crate::state::config::{GameConfig, PlayerKind};
use crate::state::game_state::{GameCtx, GameState};
use crate::state::ui_state::{Screen, UiState};
use crate::svg::paths::color_name as color_slug;

#[component]
pub fn PlayerSetup() -> Element {
    let mut config = use_signal(GameConfig::new);
    let nav = navigator();
    let mut ctx = use_context::<GameCtx>();

    let cfg = config.read().clone();
    let can_start = cfg.active_count() >= 2;

    rsx! {
        MenuScreen {
                MenuCard {
                    title: "New Game",
                    subtitle: "Set up players",
                    back: move |_| {
                            let _ = nav.push(Route::MainMenu {});
                        },
                    div { class: "player-slots",
                        for & (color , kind) in cfg.players.iter() {
                            {
                                let is_active = kind != PlayerKind::Off;
                                let color_class = format!("color-{}", color_slug(color));
                                let toggle_class = match kind {
                                    PlayerKind::Off => "player-toggle off",
                                    PlayerKind::Human => "player-toggle human",
                                    PlayerKind::Ai => "player-toggle ai",
                                };
                                rsx! {
                                    div {
                                        key: "{color:?}",
                                        class: "player-slot {color_class}",
                                        class: if is_active { "active" },
                                        ColorSwatch { color }
                                        div { class: "player-label", "{color}" }
                                        button {
                                            class: "{toggle_class}",
                                            onclick: move |_| {
                                                config.write().cycle_player(color);
                                            },
                                            "{kind.label()}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    MenuButton {
                        full_width: true,
                        disabled: !can_start,
                        on_click: move |_| {
                            let cfg = config.read();
                            let gs = GameState::new(&cfg);
                            ctx.game_state.set(gs);
                            ctx.ui_state.set(UiState::default());
                            ctx.screen.set(Screen::Playing);
                        },
                        "Start Game"
                    }
                }
        }
    }
}
