use dioxus::prelude::*;

use crate::components::title_decoration::TitleDecoration;
use crate::route::Route;
use crate::state::config::{GameConfig, PlayerKind};
use crate::state::game_state::{GameCtx, GameState};
use crate::state::ui_state::{Screen, UiState};
use crate::svg::paths::color_css_var;

#[component]
pub fn PlayerSetup() -> Element {
    let mut config = use_signal(GameConfig::new);
    let nav = navigator();
    let mut ctx = use_context::<GameCtx>();

    let cfg = config.read().clone();
    let can_start = cfg.active_count() >= 2;

    rsx! {
        div { class: "title-screen",
            TitleDecoration {}

            div { class: "title-content",
                div { class: "setup-card",
                    div { class: "hex-frame setup-hex-frame" }
                    button {
                        class: "setup-back-btn",
                        onclick: move |_| {
                            let _ = nav.push(Route::MainMenu {});
                        },
                        "\u{2190} Back"
                    }
                    div { class: "setup-title", "New Game" }
                    div { class: "setup-subtitle", "Set up players" }

                    div { class: "player-slots",
                        for & (color , kind) in cfg.players.iter() {
                            {
                                let is_active = kind != PlayerKind::Off;
                                let color_var = color_css_var(color);
                                let toggle_class = match kind {
                                    PlayerKind::Off => "player-toggle off",
                                    PlayerKind::Human => "player-toggle human",
                                    PlayerKind::Ai => "player-toggle ai",
                                };
                                rsx! {
                                    div {
                                        class: "player-slot",
                                        class: if is_active { "active" },
                                        style: "--player-color: {color_var}",
                                        div { class: "color-swatch" }
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
                    button {
                        class: "setup-start-btn",
                        class: if !can_start { "disabled" },
                        disabled: !can_start,
                        onclick: move |_| {
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
}
