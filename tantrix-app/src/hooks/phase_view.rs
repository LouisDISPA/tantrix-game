//! The board layers' view of the current turn phase, as three separate
//! memos: the empty hexes to click, the ghost preview, and the orientations
//! to choose from.
//!
//! `Board` creates all three once and hands each to the one component that
//! shows it as a `ReadSignal` prop, so each reader re-renders only when its
//! own value changes: hovering a hex moves the preview, not the hex list or
//! the orientation card. Each memo also reads only the `UiState` fields its
//! phase needs — the orientations never read `hovered` at all.

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
    /// The spot whose orientation is being chosen: marked while the card
    /// of options is open.
    pub picked: bool,
}

/// What the orientation card offers: the selected tile and the rotations
/// that fit at the selected position.
#[derive(Debug, Clone, PartialEq)]
pub struct Orientations {
    pub tile: Tile,
    pub rotations: Vec<Rotation>,
}

/// Empty hex positions to show as click targets: the hovered hand tile's
/// (faintly) while a tile is being chosen, then the selected tile's, the
/// one being decided marked. Empty while the AI plays and once the game is
/// over.
pub fn use_valid_positions() -> Memo<Vec<ValidPosition>> {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    use_memo(move || {
        let phase = ui_state.phase()();
        let (idx, faint) = match phase {
            TurnPhase::SelectTile => (ui_state.hovered()().hand_tile(), true),
            TurnPhase::SelectPosition | TurnPhase::SelectRotation => {
                (ui_state.selected_tile()(), false)
            }
            TurnPhase::WaitingForAi | TurnPhase::GameOver => return Vec::new(),
        };
        let picked = match phase {
            TurnPhase::SelectRotation => ui_state.selected_position()(),
            _ => None,
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
                picked: picked == Some(pos),
            })
            .collect()
    })
}

/// Ghost preview of the placement being considered: the hovered hand tile's
/// only move, the selected tile at the hovered (or selected) position, or at
/// the selected position in the rotation being hovered or pressed, defaulting
/// to the first valid rotation while the player chooses an option.
pub fn use_preview_placement() -> Memo<Option<Placement>> {
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;
    let dragging = use_memo(move || ui_state.tile_drag()().is_some_and(|drag| drag.is_dragging()));

    use_memo(move || {
        if dragging() {
            return None;
        }
        match ui_state.phase()() {
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
                    .filter(|rotation| rotations.contains(rotation))
                    .unwrap_or(*rotations.first()?);
                Some(Placement {
                    tile,
                    position,
                    rotation,
                })
            }

            TurnPhase::WaitingForAi | TurnPhase::GameOver => None,
        }
    })
}

/// The orientation card's tile and the rotations that fit at the selected
/// position — only during SelectRotation. Does not depend on what is
/// hovered: hovering a rotation changes the preview, not the card.
pub fn use_orientations() -> Memo<Option<Orientations>> {
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
        Some(Orientations { tile, rotations })
    })
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;
    use tantrix_game::{Placement, Position, Rotation, Tile};

    use super::{use_orientations, use_preview_placement, use_valid_positions};
    use super::{Orientations, ValidPosition};
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
        orientations: Memo<Option<Orientations>>,
    }

    fn page(game: GameState) -> GamePage {
        GamePage::local_with(game, || {
            let views = Views {
                positions: use_valid_positions(),
                preview: use_preview_placement(),
                orientations: use_orientations(),
            };
            use_context_provider(|| views);
        })
    }

    /// What the board shows: the hexes to click (by position, with their
    /// forced and faint flags), the ghost, and the orientations.
    type Shown = (
        Vec<(Position, bool, bool)>,
        Option<Placement>,
        Option<Orientations>,
    );

    fn shown(page: &GamePage) -> Shown {
        page.with(|_| {
            let views = consume_context::<Views>();
            let positions = views.positions.read();
            let positions = positions
                .iter()
                .map(|p| (p.pos, p.forced, p.faint))
                .collect();
            (
                positions,
                views.preview.cloned(),
                views.orientations.cloned(),
            )
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
        let (positions, preview, orientations) = shown(&page);
        assert_eq!(positions, [(only.position, forced, true)]);
        assert_eq!(preview, Some(only));
        assert_eq!(orientations, None);
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
    fn the_card_keeps_a_default_preview_and_shows_the_rotation_pointed_at() {
        let (game, idx, at) = tile_with_positions(|rotations| rotations.len() > 1);
        let tile = tile_at(&game, idx);
        let rotations = game.hand_info()[idx].positions[&at].clone();
        let mut page = page(game);
        page.act(|mut ctx| ctx.ui_state.select_tile(idx));
        page.act(|ctx| click_position(ctx, at));

        let offered = Some(Orientations {
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
        let (positions, preview, shown_orientations) = shown(&page);
        assert!(positions.iter().any(|&(hex, _, faint)| hex == at && !faint));
        assert_eq!(preview, ghost(rotations[0]));
        assert_eq!(shown_orientations, offered);

        hover(&mut page, Hovered::Rotation(rotations[1]));
        assert_eq!(shown(&page).1, ghost(rotations[1]));
        hover(&mut page, Hovered::Rotation(rotations[0]));
        assert_eq!(shown(&page).1, ghost(rotations[0]));
        assert_eq!(shown(&page).2, offered);

        // Letting go or a stale rotation from another spot returns to the
        // first valid option, so the tile stays visible.
        hover(&mut page, Hovered::None);
        assert_eq!(shown(&page).1, ghost(rotations[0]));
        let other = [
            Rotation::None,
            Rotation::R60,
            Rotation::R120,
            Rotation::R180,
            Rotation::R240,
            Rotation::R300,
        ]
        .into_iter()
        .find(|r| !rotations.contains(r))
        .unwrap();
        hover(&mut page, Hovered::Rotation(other));
        assert_eq!(shown(&page).1, ghost(rotations[0]));

        page.act(|mut ctx| ctx.ui_state.go_back());
        assert_eq!(shown(&page).1, None);
    }

    #[test]
    fn the_spot_being_decided_is_marked_until_the_card_closes() {
        let (game, idx, at) = tile_with_positions(|rotations| rotations.len() > 1);
        let mut page = page(game);
        let picked = |page: &GamePage| {
            page.with(|_| {
                let positions = consume_context::<Views>().positions;
                let picked: Vec<Position> = positions
                    .read()
                    .iter()
                    .filter(|p| p.picked)
                    .map(|p| p.pos)
                    .collect();
                picked
            })
        };
        page.act(|mut ctx| ctx.ui_state.select_tile(idx));
        assert!(picked(&page).is_empty());
        page.act(|ctx| click_position(ctx, at));
        assert_eq!(picked(&page), [at]);
        // Back to choosing a spot: none is decided.
        page.act(|mut ctx| ctx.ui_state.go_back());
        assert!(picked(&page).is_empty());
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
