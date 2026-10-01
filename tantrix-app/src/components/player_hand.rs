use dioxus::prelude::*;
use tantrix_game::Color;

use crate::components::tile::HandTile;
use crate::state::game_state::GameCtx;
use crate::svg::paths::color_name as color_slug;

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

/// One player's hand. Each value it shows is a memo over the game, so a move
/// re-renders the hands it changes — the mover's (a tile replaced) and those
/// whose turn starts or ends — and leaves the others alone.
#[component]
pub fn PlayerHand(color: Color, position: HandPosition) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("PlayerHand", color);
    let ctx = use_context::<GameCtx>();

    let is_current = use_memo(move || ctx.game_state.read().current_player() == color);
    let tiles = use_memo(move || ctx.game_state.read().hand(color).to_vec());
    // Player nickname (online) + whether this seat belongs to the local user.
    let player_name = use_memo(move || ctx.driver.player_name(color));
    let is_mine = use_memo(move || ctx.driver.my_color() == Some(color));

    let color_class = format!("color-{}", color_slug(color));
    let pos_class = position.css_class();
    let area_class = position.area_class();
    let tiles_class = if position.is_vertical() {
        "hand-tiles vertical"
    } else {
        "hand-tiles horizontal"
    };

    rsx! {
        div {
            class: "player-hand {pos_class} {area_class} {color_class}",
            class: if is_current() { "active" },
            class: if is_mine() { "mine" },

            div { class: "hand-header",
                if is_current() {
                    span { class: "active-dot" }
                }
                span { class: "hand-header-color", "{color}" }
                if let Some(name) = player_name() {
                    span { class: "hand-header-name", {name} }
                }
                if is_mine() {
                    span { class: "hand-header-you", "you" }
                }
            }

            if tiles.read().is_empty() {
                div { class: "hand-empty", "No tiles" }
            } else {
                div { class: tiles_class,
                    for (i, tile) in tiles.read().iter().copied().enumerate() {
                        HandTile { key: "{color:?}-{tile:?}", color, index: i }
                    }
                }
            }
        }
    }
}
