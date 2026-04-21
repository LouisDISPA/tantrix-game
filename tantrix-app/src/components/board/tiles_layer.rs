use dioxus::prelude::*;

use crate::components::tile::BoardTile;
use crate::state::game_state::{GameContext, GameStateStoreExt};
use crate::state::hooks::use_highlight_map;
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};
use crate::svg::geometry::HEX_SIZE;

/// Renders every placed move on the board with optional just-placed animation
/// and scoring-path highlighting.
#[component]
pub fn BoardTilesLayer() -> Element {
    let ctx = use_context::<GameContext>();
    let game_state = ctx.game_state;
    let ui_state = ctx.ui_state;

    let highlight_map = use_highlight_map();
    let last_placed = ui_state.last_placed_position()();
    let is_game_over = ui_state.phase()() == TurnPhase::GameOver;

    let placed_moves =
        use_memo(move || game_state.board()().iter().copied().collect::<Vec<_>>())();
    let hl_map = highlight_map();

    rsx! {
        for m in placed_moves.iter() {
            {
                let colors = hl_map.get(&m.position).cloned().unwrap_or_default();
                let just_placed = last_placed == Some(m.position);
                rsx! {
                    BoardTile {
                        key: "{m.position.x()},{m.position.y()}",
                        m: *m,
                        hex_size: HEX_SIZE,
                        just_placed,
                        dimmed: is_game_over,
                        highlight_colors: colors,
                    }
                }
            }
        }
    }
}
