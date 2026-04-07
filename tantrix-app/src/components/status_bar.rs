use dioxus::prelude::*;

use crate::state::game_state::{GameContext, GameStateStoreExt};
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};
use crate::svg::paths::color_css_var;

#[component]
pub fn StatusBar() -> Element {
    let ctx = use_context::<GameContext>();
    let gs = ctx.game_state;
    let ui = ctx.ui_state;

    // Subscribe to individual fields — won't re-render on zoom/pan/drag/hover changes
    let phase = ui.phase()();
    let message = ui.message()();

    // Subscribe to individual game fields for fine-grained reactivity
    let current_color = gs.turn_tracker().read().current();
    let color_var = color_css_var(current_color);
    let bag_count = gs.bag().read().count();
    let board_count = gs.board().read().len();

    // Subscribe to hand_info only for forced-move check
    let has_forced = gs.hand_info().read().iter().any(|info| info.has_forced);

    let phase_text = match phase {
        TurnPhase::SelectTile => {
            if has_forced {
                "Select a tile — forced move!".to_string()
            } else {
                "Select a tile from your hand".to_string()
            }
        }
        TurnPhase::SelectPosition => "Click a position on the board".to_string(),
        TurnPhase::SelectRotation => "Choose a rotation".to_string(),
        TurnPhase::WaitingForAi => format!("{} is thinking...", current_color),
        TurnPhase::GameOver => "Game Over!".to_string(),
    };

    let is_forced = has_forced && phase == TurnPhase::SelectTile;

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
                if !message.is_empty() {
                    div { class: "status-message", "{message}" }
                }
            }
        }
    }
}
