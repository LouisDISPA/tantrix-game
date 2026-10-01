//! The board layers' view of the current turn phase, as three separate
//! memos: the empty hexes to click, the ghost preview, and the rotation
//! wheel.
//!
//! `Board` creates all three once and hands each to the one component that
//! shows it as a `ReadSignal` prop, so each reader re-renders only when its
//! own value changes: hovering a hex moves the preview, not the hex list or
//! the wheel. Each memo also reads only the `UiState` fields its phase
//! needs — the wheel never reads `hovered` at all.

use dioxus::prelude::*;
use tantrix_game::{Placement, Position, Rotation, Tile};

use crate::state::actions::auto_move;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{TurnPhase, UiStateStoreExt};

/// An empty hex shown on the board as a click target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidPosition {
    pub pos: Position,
    pub forced: bool,
    /// True when the user is only *peeking* (hovered, not selected) — styled faintly.
    pub faint: bool,
}

/// Rotation wheel display data: the selected tile and the rotations that
/// fit at the selected position.
#[derive(Debug, Clone, PartialEq)]
pub struct WheelData {
    pub tile: Tile,
    pub rotations: Vec<Rotation>,
}

/// Empty hex positions to show as click targets: the hovered hand tile's
/// (faintly) while a tile is being chosen, then the selected tile's. Empty
/// while the AI plays and once the game is over.
pub fn use_valid_positions() -> Memo<Vec<ValidPosition>> {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    use_memo(move || {
        let (idx, faint) = match ui_state.phase()() {
            TurnPhase::SelectTile => (ui_state.hovered()().hand_tile(), true),
            TurnPhase::SelectPosition | TurnPhase::SelectRotation => {
                (ui_state.selected_tile()(), false)
            }
            TurnPhase::WaitingForAi | TurnPhase::GameOver => return Vec::new(),
        };
        let Some(idx) = idx else {
            return Vec::new();
        };
        let gs = ctx.game_state.read();
        let Some(info) = gs.hand_info().get(idx) else {
            return Vec::new();
        };
        info.positions
            .keys()
            .map(|&pos| ValidPosition {
                pos,
                forced: info.has_forced,
                faint,
            })
            .collect()
    })
}

/// Ghost preview of the placement being considered: the hovered hand tile's
/// only move, the selected tile at the hovered (or selected) position, or at
/// the selected position in the hovered (or first) rotation.
pub fn use_preview_placement() -> Memo<Option<Placement>> {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    use_memo(move || match ui_state.phase()() {
        TurnPhase::SelectTile => {
            // Auto-preview when a hovered tile has exactly one valid move:
            // not the opening tile, which fits every way round (a click
            // plays it in its first rotation, but no ghost shows one).
            let idx = ui_state.hovered()().hand_tile()?;
            let gs = ctx.game_state.read();
            let info = gs.hand_info().get(idx)?;
            let tile = *gs.hand(gs.current_player()).get(idx)?;
            let (position, rotation) = auto_move(info, false)?;
            Some(Placement {
                tile,
                position,
                rotation,
            })
        }

        TurnPhase::SelectPosition => {
            let idx = ui_state.selected_tile()()?;
            let hovered = ui_state.hovered()();
            let position = hovered
                .board_position()
                .or_else(|| ui_state.selected_position()())?;
            let gs = ctx.game_state.read();
            let tile = *gs.hand(gs.current_player()).get(idx)?;
            let rotations = gs.hand_info().get(idx)?.positions.get(&position)?;
            let rotation = hovered.rotation().unwrap_or(*rotations.first()?);
            Some(Placement {
                tile,
                position,
                rotation,
            })
        }

        TurnPhase::SelectRotation => {
            let idx = ui_state.selected_tile()()?;
            let position = ui_state.selected_position()()?;
            let gs = ctx.game_state.read();
            let tile = *gs.hand(gs.current_player()).get(idx)?;
            let rotations = gs.hand_info().get(idx)?.positions.get(&position)?;
            let rotation = ui_state.hovered()()
                .rotation()
                .or_else(|| rotations.first().copied())
                .unwrap_or(Rotation::None);
            Some(Placement {
                tile,
                position,
                rotation,
            })
        }

        TurnPhase::WaitingForAi | TurnPhase::GameOver => None,
    })
}

/// The rotation wheel's tile and the rotations that fit at the selected
/// position — only during SelectRotation. Does not depend on what is
/// hovered: hovering a rotation changes the preview, not the wheel.
pub fn use_rotation_wheel() -> Memo<Option<WheelData>> {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    use_memo(move || {
        if ui_state.phase()() != TurnPhase::SelectRotation {
            return None;
        }
        let idx = ui_state.selected_tile()()?;
        let position = ui_state.selected_position()()?;
        let gs = ctx.game_state.read();
        let tile = *gs.hand(gs.current_player()).get(idx)?;
        let rotations = gs.hand_info().get(idx)?.positions.get(&position)?.clone();
        Some(WheelData { tile, rotations })
    })
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;
    use tantrix_game::{Placement, Position, Rotation, Tile};

    use super::{use_preview_placement, use_rotation_wheel, use_valid_positions};
    use super::{ValidPosition, WheelData};
    use crate::state::actions::click_position;
    use crate::state::game_state::GameState;
    use crate::state::test_games::{
        fixture, fixture_after, seeded, tile_with_one_move, tile_with_positions,
    };
    use crate::state::ui_state::{Hovered, TurnPhase, UiStateStoreExt, UiStateStoreImplExt};
    use crate::util::test_page::GamePage;

    /// The three memos, as `Board` creates them for its layers.
    #[derive(Clone, Copy)]
    struct Views {
        positions: Memo<Vec<ValidPosition>>,
        preview: Memo<Option<Placement>>,
        wheel: Memo<Option<WheelData>>,
    }

    fn page(game: GameState) -> GamePage {
        GamePage::local_with(game, || {
            let views = Views {
                positions: use_valid_positions(),
                preview: use_preview_placement(),
                wheel: use_rotation_wheel(),
            };
            use_context_provider(|| views);
        })
    }

    /// What the board shows: the hexes to click (by position, with their
    /// forced and faint flags), the ghost, and the wheel.
    type Shown = (
        Vec<(Position, bool, bool)>,
        Option<Placement>,
        Option<WheelData>,
    );

    fn shown(page: &GamePage) -> Shown {
        page.with(|_| {
            let views = consume_context::<Views>();
            let positions = views.positions.read();
            let positions = positions
                .iter()
                .map(|p| (p.pos, p.forced, p.faint))
                .collect();
            (positions, views.preview.cloned(), views.wheel.cloned())
        })
    }

    fn hover(page: &mut GamePage, hovered: Hovered) {
        page.act(|ctx| ctx.ui_state.hovered().set(hovered));
    }

    /// `idx`'s positions as the board lists them, `faint` or not.
    fn positions_of(game: &GameState, idx: usize, faint: bool) -> Vec<(Position, bool, bool)> {
        let info = &game.hand_info()[idx];
        info.positions
            .keys()
            .map(|&at| (at, info.has_forced, faint))
            .collect()
    }

    fn tile_at(game: &GameState, idx: usize) -> Tile {
        game.hand(game.current_player())[idx]
    }

    #[test]
    fn a_hovered_hand_tile_shows_where_it_fits_faintly() {
        let (game, idx, _) = tile_with_positions(|_| true);
        let faint = positions_of(&game, idx, true);
        let mut page = page(game);
        assert_eq!(shown(&page), (Vec::new(), None, None));

        hover(&mut page, Hovered::HandTile(idx));
        // Several places to go: hexes to look at, but no ghost.
        assert_eq!(shown(&page), (faint, None, None));
        hover(&mut page, Hovered::None);
        assert_eq!(shown(&page), (Vec::new(), None, None));
    }

    #[test]
    fn a_hovered_tile_with_one_move_shows_its_ghost() {
        let (game, idx, only) = tile_with_one_move();
        let forced = game.hand_info()[idx].has_forced;
        let mut page = page(game);
        hover(&mut page, Hovered::HandTile(idx));
        let (positions, preview, wheel) = shown(&page);
        assert_eq!(positions, [(only.position, forced, true)]);
        assert_eq!(preview, Some(only));
        assert_eq!(wheel, None);
    }

    #[test]
    fn the_opening_tile_shows_the_centre_but_no_ghost() {
        let mut page = page(seeded(3));
        hover(&mut page, Hovered::HandTile(0));
        assert_eq!(
            shown(&page),
            (vec![(Position::CENTER, false, true)], None, None)
        );
    }

    #[test]
    fn a_forced_space_shows_as_forced() {
        // Red must fill the forced space their free move made, with the
        // fixture's fourth move.
        let game = fixture_after(3);
        let fill = &fixture().moves[3];
        let idx = game
            .hand(game.current_player())
            .iter()
            .position(|&t| t == fill.tile);
        let mut page = page(game);
        hover(&mut page, Hovered::HandTile(idx.unwrap()));
        assert_eq!(shown(&page).0, [(fill.position, true, true)]);
    }

    #[test]
    fn a_selected_tile_shows_its_hexes_and_a_ghost_on_the_hovered_one() {
        let (game, idx, at) = tile_with_positions(|_| true);
        let (tile, first) = (tile_at(&game, idx), game.hand_info()[idx].positions[&at][0]);
        let solid = positions_of(&game, idx, false);
        let mut page = page(game);
        page.act(|mut ctx| ctx.ui_state.select_tile(idx));
        assert_eq!(shown(&page), (solid.clone(), None, None));

        // Hovering another hand tile does not change what is picked.
        hover(&mut page, Hovered::HandTile(idx + 1));
        assert_eq!(shown(&page).0, solid);

        hover(&mut page, Hovered::BoardPosition(at));
        let ghost = Placement {
            tile,
            position: at,
            rotation: first,
        };
        assert_eq!(shown(&page), (solid.clone(), Some(ghost), None));

        // Where the tile does not fit, no ghost.
        let nowhere = (0..10).fold(at, |hex, _| hex.neighbor(tantrix_game::Direction::Right));
        hover(&mut page, Hovered::BoardPosition(nowhere));
        assert_eq!(shown(&page), (solid, None, None));
    }

    #[test]
    fn the_wheel_shows_the_rotations_and_the_ghost_the_hovered_one() {
        let (game, idx, at) = tile_with_positions(|rotations| rotations.len() > 1);
        let tile = tile_at(&game, idx);
        let rotations = game.hand_info()[idx].positions[&at].clone();
        let mut page = page(game);
        page.act(|mut ctx| ctx.ui_state.select_tile(idx));
        page.act(|ctx| click_position(ctx, at));

        let wheel = Some(WheelData {
            tile,
            rotations: rotations.clone(),
        });
        let ghost = |rotation: Rotation| {
            Some(Placement {
                tile,
                position: at,
                rotation,
            })
        };
        let (positions, preview, shown_wheel) = shown(&page);
        assert!(positions.iter().any(|&(hex, _, faint)| hex == at && !faint));
        assert_eq!(preview, ghost(rotations[0]));
        assert_eq!(shown_wheel, wheel);

        hover(&mut page, Hovered::Rotation(rotations[1]));
        assert_eq!(shown(&page).1, ghost(rotations[1]));
        assert_eq!(shown(&page).2, wheel);
    }

    #[test]
    fn nothing_shows_while_the_ai_plays_or_once_the_game_is_over() {
        let (game, idx, at) = tile_with_positions(|_| true);
        let page = page(game);
        for phase in [TurnPhase::WaitingForAi, TurnPhase::GameOver] {
            // Set without letting the page catch up: the AI loop would
            // hand this human game straight back.
            page.with(|mut ctx| {
                ctx.ui_state.select_tile(idx);
                ctx.ui_state.selected_position().set(Some(at));
                ctx.ui_state.hovered().set(Hovered::BoardPosition(at));
                ctx.ui_state.phase().set(phase);
            });
            assert_eq!(shown(&page), (Vec::new(), None, None), "{phase:?}");
        }
    }
}
