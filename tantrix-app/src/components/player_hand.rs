use dioxus::prelude::*;
use tantrix_game::Color;

use crate::components::tile::HandTile;
use crate::state::game_state::GameCtx;
use crate::svg::paths::color_css_var;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandPosition {
    Left,
    Right,
    Top,
    Bottom,
}

impl HandPosition {
    pub fn is_vertical(&self) -> bool {
        matches!(self, HandPosition::Left | HandPosition::Right)
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            HandPosition::Left => "side left-hand",
            HandPosition::Right => "side right-hand right",
            HandPosition::Top => "edge top-hand",
            HandPosition::Bottom => "edge bottom-hand bottom",
        }
    }

    /// CSS class encoding the grid area (paired with `.player-hand.area-*`).
    pub fn area_class(&self) -> &'static str {
        match self {
            HandPosition::Left => "area-left",
            HandPosition::Right => "area-right",
            HandPosition::Top => "area-top",
            HandPosition::Bottom => "area-bottom",
        }
    }
}

#[component]
pub fn PlayerHand(color: Color, position: HandPosition) -> Element {
    let ctx = use_context::<GameCtx>();

    let is_current = ctx.game_state.read().turn_tracker.current() == color;
    let hand_len = ctx.game_state.read().hands[color].iter().count();
    let color_var = color_css_var(color);

    let pos_class = position.css_class();
    let area_class = position.area_class();
    let tiles_class = if position.is_vertical() {
        "hand-tiles vertical"
    } else {
        "hand-tiles horizontal"
    };

    rsx! {
        div {
            class: "player-hand {pos_class} {area_class}",
            class: if is_current { "active" },
            style: "--player-color: {color_var}",

            div { class: "hand-header",
                if is_current {
                    span { class: "active-dot" }
                }
                "{color}"
            }

            if hand_len == 0 {
                div { class: "hand-empty", "No tiles" }
            } else {
                div { class: "{tiles_class}",
                    for i in 0..hand_len {
                        HandTile { key: "{color:?}-{i}", color, index: i }
                    }
                }
            }
        }
    }
}
