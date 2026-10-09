//! The board camera's arithmetic, on plain numbers.
//!
//! The viewport draws a board point `p` (pixels, as `axial_to_pixel` gives
//! them) at
//!
//! ```text
//! stage / 2 + zoom · (p − center − pan)
//! ```
//!
//! from the top-left corner of the stage, `center` being the middle of the
//! placed tiles (what the viewport centres its layer on). So `pan` is in
//! unzoomed board pixels, and a pan of `p − center` puts `p` in the middle
//! of the stage.
//!
//! Everything here is a function of plain data, tested directly; the
//! components and actions read the state, call these, and write the result
//! back to the camera fields.

use super::geometry::{Rect, HEX_SIZE, SQRT3};

/// The least and most the player can zoom by hand.
pub const MIN_ZOOM: f64 = 0.3;
pub const MAX_ZOOM: f64 = 5.0;
/// The least zoom the camera picks by itself: a legal spot is then 45px
/// across, wide enough for a finger.
pub const MIN_SPOT_ZOOM: f64 = 0.52;
/// The most it zooms in to show the choices (a tile is 87px at 1.0).
pub const MAX_FIT_ZOOM: f64 = 1.3;
/// The room kept around what the camera frames, in stage pixels.
pub const FIT_PAD: f64 = 12.0;
/// The smallest comfortable tap target, in CSS pixels.
pub const MIN_TARGET: f64 = 44.0;
/// A stage this big shows a board comfortably at 100%: a phone's, a laptop's.
/// A bigger one starts zoomed in, so the board keeps up with an interface
/// that grows with the window ([`home_zoom`]).
const HOME_STAGE: Size = Size {
    width: 700.0,
    height: 480.0,
};
/// The most the board starts zoomed in, on the biggest windows.
pub const MAX_HOME_ZOOM: f64 = 1.6;

/// A width and a height, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    /// The size of `rect`.
    pub fn of(rect: &Rect) -> Self {
        Self::new(rect.width(), rect.height())
    }
}

/// Where the camera looks: the zoom, and the pan from the placed tiles'
/// centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
}

impl Pose {
    /// 100%, the placed tiles centred.
    pub const HOME: Pose = Pose {
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
    };

    pub fn with_zoom(self, zoom: f64) -> Self {
        Self { zoom, ..self }
    }
}

/// The zoom a board starts at, and "reset" goes back to, on `stage`: 100%
/// on a phone or a laptop, more on a stage that is roomier than they are
/// (in both directions, the smaller of the two ratios), up to
/// [`MAX_HOME_ZOOM`]. A function of the stage's size alone, so the camera
/// never asks what kind of device it is on.
pub fn home_zoom(stage: Size) -> f64 {
    (stage.width / HOME_STAGE.width)
        .min(stage.height / HOME_STAGE.height)
        .clamp(1.0, MAX_HOME_ZOOM)
}

/// The most the camera zooms in by itself to show the choices on `stage`
/// ([`MAX_FIT_ZOOM`] at home, as much more as the stage's home zoom is).
pub fn max_fit_zoom(stage: Size) -> f64 {
    MAX_FIT_ZOOM * home_zoom(stage)
}

/// How wide a hex is on the screen at `zoom`.
pub fn spot_width(zoom: f64) -> f64 {
    zoom * HEX_SIZE * SQRT3
}

/// Where the board point `point` lands on the stage (from its top-left).
pub fn to_stage(point: (f64, f64), pose: Pose, center: (f64, f64), stage: Size) -> (f64, f64) {
    (
        stage.width / 2.0 + pose.zoom * (point.0 - center.0 - pose.pan_x),
        stage.height / 2.0 + pose.zoom * (point.1 - center.1 - pose.pan_y),
    )
}

/// The board point drawn at `screen` on the stage: [`to_stage`]'s inverse.
#[cfg(test)]
pub fn to_board(screen: (f64, f64), pose: Pose, center: (f64, f64), stage: Size) -> (f64, f64) {
    (
        (screen.0 - stage.width / 2.0) / pose.zoom + center.0 + pose.pan_x,
        (screen.1 - stage.height / 2.0) / pose.zoom + center.1 + pose.pan_y,
    )
}

/// Where the board rectangle `rect` lands on the stage.
pub fn rect_to_stage(rect: Rect, pose: Pose, center: (f64, f64), stage: Size) -> Rect {
    let (min_x, min_y) = to_stage((rect.min_x, rect.min_y), pose, center, stage);
    let (max_x, max_y) = to_stage((rect.max_x, rect.max_y), pose, center, stage);
    Rect::new(min_x, min_y, max_x, max_y)
}

/// The pose that shows the board rectangle `view` whole and centred, with
/// `pad` pixels to spare, the zoom kept within `min_zoom..=max_zoom`.
pub fn fit(
    view: Rect,
    center: (f64, f64),
    stage: Size,
    pad: f64,
    min_zoom: f64,
    max_zoom: f64,
) -> Pose {
    let zoom = ((stage.width - 2.0 * pad) / view.width())
        .min((stage.height - 2.0 * pad) / view.height())
        .clamp(min_zoom, max_zoom);
    let (x, y) = view.center();
    Pose {
        zoom,
        pan_x: x - center.0,
        pan_y: y - center.1,
    }
}

/// Whether `view` is wholly on the stage (inside it by `pad`) at `pose`.
pub fn shows(view: Rect, pose: Pose, center: (f64, f64), stage: Size, pad: f64) -> bool {
    stage_rect(stage, pad).contains(&rect_to_stage(view, pose, center, stage))
}

fn stage_rect(stage: Size, pad: f64) -> Rect {
    Rect::new(pad, pad, stage.width - pad, stage.height - pad)
}

/// `pose` panned so the board moves by (`dx`, `dy`) screen pixels: what a
/// finger dragging the board does.
pub fn pan_by(pose: Pose, dx: f64, dy: f64) -> Pose {
    Pose {
        pan_x: pose.pan_x - dx / pose.zoom,
        pan_y: pose.pan_y - dy / pose.zoom,
        ..pose
    }
}

/// `pose` zoomed by `factor` about `pivot` (a stage point), so the board
/// point under it stays under it. The zoom stays within `min..=max`.
pub fn pinch(pose: Pose, pivot: (f64, f64), stage: Size, factor: f64, min: f64, max: f64) -> Pose {
    let zoom = (pose.zoom * factor).clamp(min, max);
    let from_middle = (pivot.0 - stage.width / 2.0, pivot.1 - stage.height / 2.0);
    let k = 1.0 / pose.zoom - 1.0 / zoom;
    Pose {
        zoom,
        pan_x: pose.pan_x + from_middle.0 * k,
        pan_y: pose.pan_y + from_middle.1 * k,
    }
}

/// The camera for a hand tile that has just been selected, `spots` being
/// the board rectangle of the places it fits and `placed` that of the
/// tiles on the board. Leaves the camera alone while every spot is on the
/// stage and wide enough to tap; otherwise it frames the tiles and the
/// spots, and the spots alone when the tiles are too spread out to show.
pub fn on_select(
    placed: Option<Rect>,
    spots: Rect,
    pose: Pose,
    center: (f64, f64),
    stage: Size,
) -> Pose {
    if shows(spots, pose, center, stage, FIT_PAD) && spot_width(pose.zoom) >= MIN_TARGET {
        return pose;
    }
    let frame = |view: Rect| {
        fit(
            view,
            center,
            stage,
            FIT_PAD,
            MIN_SPOT_ZOOM,
            max_fit_zoom(stage),
        )
    };
    let all = placed.map_or(spots, |placed| placed.union(spots));
    let framed = frame(all);
    if shows(all, framed, center, stage, FIT_PAD) {
        framed
    } else {
        frame(spots)
    }
}

/// The camera once a move has been played, the placed tiles' rectangle
/// being `placed`: back in the middle (the player's zoom stays) — zoomed
/// out, never below [`MIN_SPOT_ZOOM`], when the board no longer fits.
pub fn after_move(placed: Rect, zoom: f64, center: (f64, f64), stage: Size) -> Pose {
    let pose = Pose::HOME.with_zoom(zoom);
    if shows(placed, pose, center, stage, FIT_PAD) {
        return pose;
    }
    let fitted = fit(
        placed,
        center,
        stage,
        FIT_PAD,
        MIN_SPOT_ZOOM,
        max_fit_zoom(stage),
    );
    pose.with_zoom(zoom.min(fitted.zoom))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAGE: Size = Size {
        width: 390.0,
        height: 600.0,
    };

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
    }

    #[test]
    fn a_spot_stays_wide_enough_to_tap_at_the_least_zoom_the_camera_picks() {
        assert!(spot_width(MIN_SPOT_ZOOM) >= MIN_TARGET);
        assert!(spot_width(MIN_ZOOM) < MIN_TARGET);
        assert!((spot_width(1.0) - 86.6).abs() < 0.1);
    }

    #[test]
    fn a_phone_or_laptop_stage_starts_at_100_percent_and_a_roomy_one_zooms_in() {
        // A phone, in portrait and landscape; a laptop with a side panel.
        for stage in [STAGE, Size::new(612.0, 340.0), Size::new(985.0, 474.0)] {
            assert_eq!(home_zoom(stage), 1.0, "{stage:?}");
            assert_eq!(max_fit_zoom(stage), MAX_FIT_ZOOM, "{stage:?}");
        }
        // A full HD window's board is bigger, by its tighter side.
        let hd = Size::new(1477.0, 729.0);
        assert!((home_zoom(hd) - 729.0 / 480.0).abs() < 1e-9);
        assert!(max_fit_zoom(hd) > MAX_FIT_ZOOM * 1.5);
        // Never above the ceiling, however big the window.
        assert_eq!(home_zoom(Size::new(5000.0, 3000.0)), MAX_HOME_ZOOM);
        // A stage that is wide but short is held back by its height.
        assert_eq!(home_zoom(Size::new(3000.0, 400.0)), 1.0);
    }

    #[test]
    fn the_centre_is_drawn_in_the_middle_of_the_stage() {
        let center = (30.0, -20.0);
        assert!(close(
            to_stage(center, Pose::HOME, center, STAGE),
            (195.0, 300.0)
        ));
        let pose = Pose {
            zoom: 2.0,
            pan_x: 10.0,
            pan_y: 0.0,
        };
        // 100px right of the centre: 2 · (100 − 10) from the middle.
        assert!(close(
            to_stage((130.0, -20.0), pose, center, STAGE),
            (195.0 + 180.0, 300.0)
        ));
    }

    #[test]
    fn the_stage_and_the_board_map_back_and_forth() {
        let pose = Pose {
            zoom: 0.7,
            pan_x: -35.0,
            pan_y: 12.5,
        };
        let center = (-80.0, 41.0);
        for p in [(0.0, 0.0), (120.0, -300.0), (-77.7, 9.1)] {
            let on_stage = to_stage(p, pose, center, STAGE);
            assert!(close(to_board(on_stage, pose, center, STAGE), p));
        }
    }

    #[test]
    fn fitting_centres_the_view_and_clamps_the_zoom() {
        let center = (0.0, 0.0);
        let view = Rect::new(100.0, 0.0, 300.0, 200.0);
        let pose = fit(view, center, STAGE, 12.0, 0.3, 5.0);
        // 366px of stage for 200px of board.
        assert!((pose.zoom - 366.0 / 200.0).abs() < 1e-9);
        assert!(close((pose.pan_x, pose.pan_y), (200.0, 100.0)));
        assert!(shows(view, pose, center, STAGE, 12.0));
        // The middle of the view is the middle of the stage.
        assert!(close(
            to_stage((200.0, 100.0), pose, center, STAGE),
            (195.0, 300.0)
        ));

        let capped = fit(view, center, STAGE, 12.0, 0.3, 1.3);
        assert_eq!(capped.zoom, 1.3);
        let wide = Rect::new(0.0, 0.0, 5000.0, 100.0);
        assert_eq!(fit(wide, center, STAGE, 12.0, 0.52, 1.3).zoom, 0.52);
    }

    #[test]
    fn dragging_moves_the_board_by_the_distance_dragged() {
        let pose = Pose {
            zoom: 1.5625,
            pan_x: 4.0,
            pan_y: -2.0,
        };
        let center = (10.0, 10.0);
        let p = (55.0, -12.0);
        let before = to_stage(p, pose, center, STAGE);
        let after = to_stage(p, pan_by(pose, 31.25, -15.625), center, STAGE);
        assert!(close(after, (before.0 + 31.25, before.1 - 15.625)));
        // The same arithmetic as the mouse drag: pan −= distance / zoom.
        let panned = pan_by(Pose::HOME.with_zoom(1.5625), 31.25, -5.625);
        assert!(close((panned.pan_x, panned.pan_y), (-20.0, 3.6)));
    }

    #[test]
    fn pinching_keeps_the_board_under_the_fingers() {
        let center = (-80.0, 41.0);
        let pose = Pose {
            zoom: 0.8,
            pan_x: 20.0,
            pan_y: -30.0,
        };
        let pivot = (310.0, 120.0);
        for factor in [0.5, 1.0, 1.7, 3.0] {
            let pinched = pinch(pose, pivot, STAGE, factor, MIN_ZOOM, MAX_ZOOM);
            assert!((pinched.zoom - (0.8 * factor).clamp(MIN_ZOOM, MAX_ZOOM)).abs() < 1e-9);
            assert!(close(
                to_board(pivot, pinched, center, STAGE),
                to_board(pivot, pose, center, STAGE)
            ));
        }
        // Pinching about the middle only zooms.
        let middle = pinch(pose, (195.0, 300.0), STAGE, 2.0, MIN_ZOOM, MAX_ZOOM);
        assert_eq!((middle.pan_x, middle.pan_y), (20.0, -30.0));
        // And stops at its limits.
        assert_eq!(pinch(pose, pivot, STAGE, 100.0, 0.3, 5.0).zoom, 5.0);
        assert_eq!(pinch(pose, pivot, STAGE, 0.001, 0.3, 5.0).zoom, 0.3);
    }

    #[test]
    fn choosing_a_tile_leaves_a_camera_that_shows_its_spots_alone() {
        let center = (0.0, 0.0);
        let placed = Rect::new(-90.0, -100.0, 90.0, 100.0);
        let spots = Rect::new(-170.0, -100.0, 170.0, 100.0);
        let pose = Pose {
            zoom: 0.9,
            pan_x: 5.0,
            pan_y: -5.0,
        };
        assert_eq!(on_select(Some(placed), spots, pose, center, STAGE), pose);
    }

    #[test]
    fn choosing_a_tile_frames_spots_that_are_off_the_stage_or_too_small() {
        let center = (0.0, 0.0);
        let placed = Rect::new(-90.0, -100.0, 90.0, 100.0);
        let spots = Rect::new(-170.0, -100.0, 170.0, 100.0);

        // Spots out of sight: frame the tiles and the spots.
        let off = Pose::HOME.with_zoom(2.0);
        let framed = on_select(Some(placed), spots, off, center, STAGE);
        assert!(shows(spots, framed, center, STAGE, FIT_PAD));
        assert!(shows(placed, framed, center, STAGE, FIT_PAD));
        assert!((MIN_SPOT_ZOOM..=MAX_FIT_ZOOM).contains(&framed.zoom));

        // Spots in sight but too small to tap.
        let small = Pose::HOME.with_zoom(MIN_ZOOM);
        let framed = on_select(Some(placed), spots, small, center, STAGE);
        assert!(spot_width(framed.zoom) >= MIN_TARGET);

        // No tiles to keep in view (the opening move): the spots alone.
        let alone = on_select(None, spots, off, center, STAGE);
        assert!(shows(spots, alone, center, STAGE, FIT_PAD));
    }

    #[test]
    fn spots_far_from_the_tiles_are_kept_on_the_stage_when_everything_cannot_be() {
        let center = (0.0, 0.0);
        // A chain far wider than the stage at the least zoom; the spots at
        // one end.
        let placed = Rect::new(-2000.0, -50.0, 2000.0, 50.0);
        let spots = Rect::new(1900.0, -50.0, 2100.0, 50.0);
        let pose = on_select(Some(placed), spots, Pose::HOME, center, STAGE);
        assert_eq!(pose.zoom, MAX_FIT_ZOOM);
        assert!(shows(spots, pose, center, STAGE, FIT_PAD));
    }

    #[test]
    fn after_a_move_the_board_is_recentred_at_the_players_zoom_if_it_fits() {
        let placed = Rect::new(-120.0, -100.0, 120.0, 100.0);
        let pose = after_move(placed, 1.4, (0.0, 0.0), STAGE);
        assert_eq!(pose, Pose::HOME.with_zoom(1.4));
    }

    #[test]
    fn after_a_move_a_board_that_no_longer_fits_is_zoomed_out_to_fit() {
        let center = (0.0, 0.0);
        let placed = Rect::new(-300.0, -150.0, 300.0, 150.0);
        let pose = after_move(placed, 1.0, center, STAGE);
        assert!(pose.zoom < 1.0);
        assert!(shows(placed, pose, center, STAGE, FIT_PAD));
        assert_eq!((pose.pan_x, pose.pan_y), (0.0, 0.0));

        // Never below what keeps a spot tappable, even if it still overflows.
        let sprawl = Rect::new(-4000.0, -200.0, 4000.0, 200.0);
        assert_eq!(after_move(sprawl, 1.0, center, STAGE).zoom, MIN_SPOT_ZOOM);
        // A player already zoomed out further keeps their zoom.
        assert_eq!(after_move(sprawl, 0.4, center, STAGE).zoom, 0.4);
    }

    #[test]
    fn a_roomy_stage_frames_the_choices_at_a_larger_zoom() {
        let hd = Size::new(1477.0, 729.0);
        let center = (0.0, 0.0);
        // One tile, its spots off to the side of a camera that looks elsewhere.
        let placed = Rect::new(-45.0, -50.0, 45.0, 50.0);
        let spots = Rect::new(120.0, -50.0, 300.0, 50.0);
        let far = Pose::HOME.with_zoom(3.0);
        let pose = on_select(Some(placed), spots, far, center, hd);
        assert!(
            pose.zoom > MAX_FIT_ZOOM && pose.zoom <= max_fit_zoom(hd),
            "{pose:?}"
        );
        assert!(shows(spots, pose, center, hd, FIT_PAD));
    }
}
