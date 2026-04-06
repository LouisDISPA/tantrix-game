use dioxus::prelude::*;

use crate::components::game_screen::GameContext;
use crate::state::ui_state::TurnPhase;
use crate::svg::paths::color_css_var;

#[component]
pub fn StatusBar() -> Element {
    let ctx = use_context::<GameContext>();
    let gs = ctx.game_state.read();
    let ui = ctx.ui_state.read();

    let current_color = gs.currently_playing();
    let color_var = color_css_var(current_color);
    let bag_count = gs.bag_count();
    let board_count = gs.board_moves().len();

    let phase_text = match ui.phase {
        TurnPhase::SelectTile => {
            if gs.has_forced_moves() {
                "Select a tile — forced move!".to_string()
            } else {
                "Select a tile from your hand".to_string()
            }
        }
        TurnPhase::SelectPosition => "Click a position on the board".to_string(),
        TurnPhase::SelectRotation => "Choose a rotation".to_string(),
        TurnPhase::GameOver => "Game Over!".to_string(),
    };

    let is_forced = gs.has_forced_moves() && ui.phase == TurnPhase::SelectTile;

    rsx! {
        div { class: "status-bar",
            // Turn banner — full-width colored strip
            div {
                display: "flex",
                align_items: "center",
                justify_content: "center",
                div {
                    class: if is_forced { "turn-banner forced" } else { "turn-banner" },
                    style: "--turn-color: {color_var}",
                    span { class: "turn-banner-name", "{current_color}" }
                    span { class: "turn-banner-phase", "{phase_text}" }
                }
            }
            // Info pills
            div { class: "status-pills",
                div { class: "status-item", "Bag: {bag_count}" }
                div { class: "status-item", "Board: {board_count}" }
                if !ui.message.is_empty() {
                    div { class: "status-message", "{ui.message}" }
                }
            }
        }
    }
}
