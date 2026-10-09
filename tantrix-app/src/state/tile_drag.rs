//! A hand tile's pointer gesture. The game changes only on a legal drop.

use dioxus::prelude::*;
use tantrix_game::{Color, Position, Rotation, Tile};

use super::actions::click_position;
use super::game_state::{GameCtx, GameState};
use super::gesture::Pointer;
use super::ui_state::{set_if_changed, Hovered, TurnPhase, UiStateStoreExt, UiStateStoreImplExt};
use crate::hooks::camera::BoardCamera;
use crate::svg::camera_math::{spot_width, to_stage, Size};
use crate::svg::geometry::{axial_to_pixel, board_center, Rect, HEX_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragPhase {
    Pressed,
    Dragging,
    Returning,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragTarget {
    pub position: Position,
    pub center: (f64, f64),
    pub width: f64,
    pub rotation: Rotation,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileDrag {
    pub color: Color,
    pub index: usize,
    pub tile: Tile,
    pub pointer: Pointer,
    pub origin: Rect,
    pub start: (f64, f64),
    pub slop: f64,
    pub phase: DragPhase,
    pub target: Option<DragTarget>,
    board_len: usize,
}

impl TileDrag {
    pub fn is_dragging(self) -> bool {
        self.phase == DragPhase::Dragging
    }

    pub fn holds_pointer(self) -> bool {
        self.phase != DragPhase::Returning
    }

    /// A snapshot must still describe the hand and turn we picked up from.
    pub fn is_current(self, game: &GameState) -> bool {
        game.current_player() == self.color
            && game.board().len() == self.board_len
            && game.hand(self.color).get(self.index) == Some(&self.tile)
    }
}

/// Arm the gesture without selecting, auto-playing, or moving the camera.
pub fn press_tile(
    ctx: GameCtx,
    color: Color,
    index: usize,
    pointer: Pointer,
    origin: Rect,
    slop: f64,
) {
    let ui = ctx.ui_state;
    if ui.tile_drag().peek().is_some_and(TileDrag::holds_pointer)
        || !ctx.driver.controls(color)
        || matches!(
            *ui.phase().peek(),
            TurnPhase::WaitingForAi | TurnPhase::GameOver
        )
    {
        return;
    }
    let game = ctx.game_state.peek();
    let Some(info) = game.hand_info().get(index) else {
        return;
    };
    if game.current_player() != color
        || !info.has_moves()
        || (game.hand_info().iter().any(|i| i.has_forced) && !info.has_forced)
    {
        return;
    }
    let Some(&tile) = game.hand(color).get(index) else {
        return;
    };
    ui.suppress_tile_click().set(false);
    ui.tile_drag().set(Some(TileDrag {
        color,
        index,
        tile,
        pointer,
        origin,
        start: (pointer.x, pointer.y),
        slop,
        phase: DragPhase::Pressed,
        target: None,
        board_len: game.board().len(),
    }));
}

pub fn move_tile(ctx: GameCtx, pointer: Pointer) {
    let mut ui = ctx.ui_state;
    let Some(mut drag) = *ui.tile_drag().peek() else {
        return;
    };
    if drag.pointer.id != pointer.id || !drag.holds_pointer() {
        return;
    }
    if !drag.is_current(&ctx.game_state.peek()) || !ctx.driver.controls(drag.color) {
        cancel_tile_drag(ctx);
        return;
    }
    drag.pointer = pointer;
    if drag.phase == DragPhase::Pressed {
        if (pointer.x - drag.start.0).hypot(pointer.y - drag.start.1) <= drag.slop {
            return;
        }
        // Unlike click_tile, selection here never auto-plays or reframes.
        ui.select_tile(drag.index);
        ui.suppress_tile_click().set(true);
        drag.phase = DragPhase::Dragging;
    }
    drag.target = target_of(ctx, drag);
    set_if_changed(ui.tile_drag(), Some(drag));
}

/// Release uses the same position/rotation decision as a click. A tap leaves
/// that decision to the browser's ensuing click; a drag consumes that click.
pub fn drop_tile(ctx: GameCtx, pointer: Pointer) {
    move_tile(ctx, pointer);
    let ui = ctx.ui_state;
    let Some(drag) = *ui.tile_drag().peek() else {
        return;
    };
    if drag.pointer.id != pointer.id || !drag.holds_pointer() {
        return;
    }
    if drag.phase == DragPhase::Pressed {
        ui.tile_drag().set(None);
        return;
    }
    let Some(target) = drag.target else {
        cancel_tile_drag(ctx);
        return;
    };
    if !drag.is_current(&ctx.game_state.peek()) || !ctx.driver.controls(drag.color) {
        cancel_tile_drag(ctx);
        return;
    }
    ui.tile_drag().set(None);
    click_position(ctx, target.position);
    if *ui.phase().peek() == TurnPhase::SelectRotation {
        // Keep the dropped tile visible while the player chooses its way round.
        ui.hovered().set(Hovered::Rotation(target.rotation));
    }
}

pub fn cancel_tile_drag(ctx: GameCtx) {
    let mut ui = ctx.ui_state;
    let Some(mut drag) = *ui.tile_drag().peek() else {
        return;
    };
    if !drag.holds_pointer() {
        return;
    }
    ui.suppress_tile_click().set(true);
    if drag.is_dragging() {
        ui.reset_selection();
        drag.phase = DragPhase::Returning;
        drag.target = None;
        ui.tile_drag().set(Some(drag));
    } else {
        ui.tile_drag().set(None);
    }
}

pub fn holds_tile_pointer(ctx: GameCtx) -> bool {
    ctx.ui_state
        .tile_drag()
        .peek()
        .is_some_and(TileDrag::holds_pointer)
}

fn target_of(ctx: GameCtx, drag: TileDrag) -> Option<DragTarget> {
    let stage = (*ctx.ui_state.stage().peek())?;
    let at = (drag.pointer.x, drag.pointer.y);
    // Cards surround the stage; releasing over the rack or HUD is a cancel.
    if at.0 < stage.min_x || at.0 > stage.max_x || at.1 < stage.min_y || at.1 > stage.max_y {
        return None;
    }
    let game = ctx.game_state.peek();
    let info = game.hand_info().get(drag.index)?;
    let placed: Vec<_> = game.board().iter().map(|m| m.position).collect();
    let center = board_center(&placed, HEX_SIZE);
    let pose = BoardCamera::new(ctx.ui_state).pose();
    let targets = info.positions.iter().filter_map(|(&position, rotations)| {
        let (x, y) = to_stage(
            axial_to_pixel(position, HEX_SIZE),
            pose,
            center,
            Size::of(&stage),
        );
        Some(DragTarget {
            position,
            center: (x + stage.min_x, y + stage.min_y),
            width: spot_width(pose.zoom),
            rotation: *rotations.first()?,
        })
    });
    snap_target(
        at,
        targets,
        drag.target.map(|t| t.position),
        (HEX_SIZE * pose.zoom * 0.9).max(22.0),
    )
}

/// A little magnetic hold prevents a small wobble from breaking the snap.
fn snap_target(
    at: (f64, f64),
    targets: impl Iterator<Item = DragTarget>,
    previous: Option<Position>,
    radius: f64,
) -> Option<DragTarget> {
    let mut nearest = None;
    let mut best = radius;
    for target in targets {
        let distance = (target.center.0 - at.0).hypot(target.center.1 - at.1);
        if previous == Some(target.position) && distance <= (radius * 1.2).min(target.width * 0.6) {
            return Some(target);
        }
        if distance <= best {
            best = distance;
            nearest = Some(target);
        }
    }
    nearest
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Direction, EnumMap};
    use tantrix_net::SessionCommand;

    use super::*;
    use crate::state::actions::{click_tile, pick_rotation};
    use crate::state::test_games::{
        fixture_after, seeded, tile_with_one_move, tile_with_positions,
    };
    use crate::util::test_page::GamePage;

    const STAGE: Rect = Rect {
        min_x: 40.0,
        min_y: 50.0,
        max_x: 940.0,
        max_y: 650.0,
    };
    const ORIGIN: Rect = Rect {
        min_x: 400.0,
        min_y: 680.0,
        max_x: 460.0,
        max_y: 750.0,
    };

    fn press(page: &mut GamePage, index: usize) {
        page.act(|ctx| {
            ctx.ui_state.stage().set(Some(STAGE));
            press_tile(
                ctx,
                ctx.game_state.peek().current_player(),
                index,
                Pointer::new(1, 430.0, 715.0),
                ORIGIN,
                10.0,
            );
        });
    }

    fn at(page: &GamePage, position: Position) -> Pointer {
        page.with(|ctx| {
            let game = ctx.game_state.peek();
            let placed: Vec<_> = game.board().iter().map(|m| m.position).collect();
            let (x, y) = to_stage(
                axial_to_pixel(position, HEX_SIZE),
                BoardCamera::new(ctx.ui_state).pose(),
                board_center(&placed, HEX_SIZE),
                Size::of(&STAGE),
            );
            Pointer::new(1, x + STAGE.min_x, y + STAGE.min_y)
        })
    }

    #[test]
    fn a_tap_keeps_the_click_path_and_a_drag_never_auto_plays_on_pickup() {
        let (game, index, only) = tile_with_one_move();
        let color = game.current_player();
        let before = game.board().len();
        let mut page = GamePage::local(game);
        press(&mut page, index);
        page.act(|ctx| drop_tile(ctx, Pointer::new(1, 434.0, 716.0)));
        assert_eq!(page.game(|g| g.board().len()), before);
        assert!(!page.with(|ctx| *ctx.ui_state.suppress_tile_click().peek()));
        page.act(|ctx| click_tile(ctx, color, index));
        assert!(page.game(|g| g.board().iter().any(|m| m.placement() == only)));

        let (game, index, _) = tile_with_one_move();
        let before = game.board().len();
        let mut page = GamePage::local(game);
        press(&mut page, index);
        page.act(|ctx| move_tile(ctx, Pointer::new(1, 470.0, 690.0)));
        assert_eq!(page.game(|g| g.board().len()), before);
        assert_eq!(
            page.with(|ctx| *ctx.ui_state.phase().peek()),
            TurnPhase::SelectPosition
        );
        page.act(|ctx| drop_tile(ctx, Pointer::new(1, 470.0, 690.0)));
        assert_eq!(page.game(|g| g.board().len()), before);
        assert_eq!(
            page.with(|ctx| ctx.ui_state.tile_drag().peek().unwrap().phase),
            DragPhase::Returning
        );
    }

    #[test]
    fn a_legal_drop_accounts_for_pan_zoom_and_stage_offset() {
        for zoom in [0.52, 1.0, 1.6] {
            let (game, index, only) = tile_with_one_move();
            let before = game.board().len();
            let mut page = GamePage::local(game);
            press(&mut page, index);
            page.act(|ctx| {
                ctx.ui_state.zoom().set(zoom);
                ctx.ui_state.pan_x().set(20.0);
                ctx.ui_state.pan_y().set(-10.0);
            });
            let pointer = at(&page, only.position);
            page.act(|ctx| drop_tile(ctx, pointer));
            assert_eq!(page.game(|g| g.board().len()), before + 1);
            assert!(page.game(|g| g.board().iter().any(|m| m.placement() == only)));
        }
    }

    #[test]
    fn a_drop_with_several_rotations_previews_the_tile_and_opens_the_picker() {
        let (game, index, position) = tile_with_positions(|rotations| rotations.len() > 1);
        let before = game.board().len();
        let rotation = game.hand_info()[index].positions[&position][1];
        let mut page = GamePage::local(game);
        press(&mut page, index);
        let pointer = at(&page, position);
        page.act(|ctx| drop_tile(ctx, pointer));
        assert_eq!(page.game(|g| g.board().len()), before);
        assert_eq!(
            page.with(|ctx| *ctx.ui_state.phase().peek()),
            TurnPhase::SelectRotation
        );
        assert!(page.with(|ctx| matches!(*ctx.ui_state.hovered().peek(), Hovered::Rotation(_))));
        page.act(|ctx| pick_rotation(ctx, rotation));
        assert_eq!(page.game(|g| g.board().len()), before + 1);
    }

    #[test]
    fn an_online_drop_sends_exactly_one_move_and_waits_for_the_room() {
        let (game, index, only) = tile_with_one_move();
        let before = game.board().len();
        let mover = game.current_player();
        let (mut page, mut commands) = GamePage::online(game, Some(mover), EnumMap::default());
        press(&mut page, index);
        let pointer = at(&page, only.position);
        page.act(|ctx| drop_tile(ctx, pointer));
        page.act(|ctx| drop_tile(ctx, pointer));
        assert!(matches!(commands.try_recv(), Ok(SessionCommand::PlayMove(p)) if p == only));
        assert!(commands.try_recv().is_err());
        assert_eq!(page.game(|g| g.board().len()), before);
    }

    #[test]
    fn disabled_tiles_other_seats_and_other_pointers_cannot_drag() {
        let game = fixture_after(3);
        let disabled = game.hand_info().iter().position(|i| !i.has_forced).unwrap();
        let forced = game.hand_info().iter().position(|i| i.has_forced).unwrap();
        let mut page = GamePage::local(game.clone());
        press(&mut page, disabled);
        assert!(page.with(|ctx| ctx.ui_state.tile_drag().peek().is_none()));
        let (mut spectator, _) = GamePage::online(game, None, EnumMap::default());
        press(&mut spectator, forced);
        assert!(spectator.with(|ctx| ctx.ui_state.tile_drag().peek().is_none()));
        press(&mut page, forced);
        page.act(|ctx| drop_tile(ctx, Pointer::new(2, 400.0, 300.0)));
        assert_eq!(
            page.with(|ctx| ctx.ui_state.tile_drag().peek().unwrap().phase),
            DragPhase::Pressed
        );
        page.act(cancel_tile_drag);
        assert!(page.with(|ctx| ctx.ui_state.tile_drag().peek().is_none()));
    }

    #[test]
    fn a_changed_snapshot_discards_the_drag() {
        let mut page = GamePage::local(seeded(3));
        press(&mut page, 0);
        page.act(|mut ctx| ctx.game_state.set(seeded(9)));
        assert!(page.with(|ctx| ctx.ui_state.tile_drag().peek().is_none()));
    }

    #[test]
    fn snapping_has_a_bounded_radius_and_a_small_magnetic_hold() {
        let center = Position::CENTER;
        let right = center.neighbor(Direction::Right);
        let target = |position, x, width| DragTarget {
            position,
            center: (x, 100.0),
            width,
            rotation: Rotation::None,
        };
        let targets = [target(center, 100.0, 86.0), target(right, 186.0, 86.0)];
        assert_eq!(
            snap_target((138.0, 100.0), targets.into_iter(), None, 45.0)
                .unwrap()
                .position,
            center
        );
        assert_eq!(
            snap_target((148.0, 100.0), targets.into_iter(), Some(center), 45.0)
                .unwrap()
                .position,
            center
        );
        assert_eq!(
            snap_target((165.0, 100.0), targets.into_iter(), Some(center), 45.0)
                .unwrap()
                .position,
            right
        );
        assert!(snap_target((400.0, 100.0), targets.into_iter(), None, 45.0).is_none());
        let tiny = [target(center, 100.0, 26.0), target(right, 126.0, 26.0)];
        assert_eq!(
            snap_target((126.0, 100.0), tiny.into_iter(), Some(center), 22.0)
                .unwrap()
                .position,
            right
        );
    }
}
