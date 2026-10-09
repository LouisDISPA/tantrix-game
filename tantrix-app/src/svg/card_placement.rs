//! Where the orientation card sits when it pops up at a spot.
//!
//! The card is a neutral box with the tile's two ways of lying on the spot.
//! It is drawn in stage pixels (it keeps its size whatever the zoom), on
//! whichever side of the spot — above, below, left or right — hides the
//! fewest tiles. Everything is measured on the stage, so the board's
//! camera is already applied: [`rect_to_stage`] turns the spot and the tiles
//! into the rectangles passed here.

use tantrix_game::Position;

use super::camera_math::{max_fit_zoom, rect_to_stage, Pose, Size, MIN_SPOT_ZOOM};
use super::geometry::{board_center, hex_rect, Rect, HEX_SIZE};

/// The card's size: two 78×84 options, 8px apart, in 8px of padding. The
/// stylesheet gets these through `--card-w` / `--card-h`
/// ([`CardPlacement::style`]), so the numbers live here only.
pub const CARD_WIDTH: f64 = 180.0;
pub const CARD_HEIGHT: f64 = 100.0;
pub const CARD: Size = Size {
    width: CARD_WIDTH,
    height: CARD_HEIGHT,
};

/// Between the spot and the card.
const GAP: f64 = 12.0;
/// The card keeps this far from the stage's edges.
const MARGIN_X: f64 = 8.0;
const MARGIN_Y: f64 = 6.0;
/// A tile counts by its middle: its outer 12% on each side may be covered.
const TILE_SHRINK: f64 = 0.12;
/// Covering the spot itself costs this many times what covering a tile does:
/// in practice, never.
const SPOT_PENALTY: f64 = 1000.0;
/// The arrow stays this far from the card's corners.
const ARROW_INSET: f64 = 14.0;
/// How far along its side the card may slide, as a share of the most it
/// can while its arrow still points at the spot; centred first.
const SLIDES: [f64; 5] = [0.0, -0.5, 0.5, -1.0, 1.0];

/// Which side of the spot the card sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

impl Side {
    /// In the order a tie between two sides is settled.
    pub const ALL: [Side; 4] = [Side::Top, Side::Bottom, Side::Left, Side::Right];

    /// The class that points the card's arrow at the spot.
    pub fn css_class(self) -> &'static str {
        match self {
            Side::Top => "side-top",
            Side::Bottom => "side-bottom",
            Side::Left => "side-left",
            Side::Right => "side-right",
        }
    }
}

/// Where the card goes, in stage pixels from the stage's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardPlacement {
    pub side: Side,
    pub left: f64,
    pub top: f64,
    /// Where along the card's top/bottom edge, and along its left/right
    /// edge, the arrow points at the spot from.
    pub arrow_x: f64,
    pub arrow_y: f64,
}

impl CardPlacement {
    /// The card's rectangle on the stage.
    #[cfg(test)]
    pub fn rect(&self, card: Size) -> Rect {
        Rect::at(self.left, self.top, card.width, card.height)
    }

    /// The card's inline style: its position, arrow and size.
    pub fn style(&self) -> String {
        format!(
            "left: {:.1}px; top: {:.1}px; --arrow-x: {:.1}px; --arrow-y: {:.1}px; {}",
            self.left,
            self.top,
            self.arrow_x,
            self.arrow_y,
            size_style()
        )
    }
}

/// The top-left corner of a `card` on `side` of `spot`, before it is kept
/// on the stage.
fn origin(side: Side, spot: Rect, card: Size) -> (f64, f64) {
    let (cx, cy) = spot.center();
    match side {
        Side::Top => (cx - card.width / 2.0, spot.min_y - card.height - GAP),
        Side::Bottom => (cx - card.width / 2.0, spot.max_y + GAP),
        Side::Left => (spot.min_x - card.width - GAP, cy - card.height / 2.0),
        Side::Right => (spot.max_x + GAP, cy - card.height / 2.0),
    }
}

/// The side of `spot` that hides the fewest of `tiles` (all on the stage).
/// Each side's card is first moved onto the stage; what it then covers of
/// the tiles, plus a heavy price for covering the spot, is its cost. The
/// cheapest wins, ties going to the first of [`Side::ALL`], and a card that
/// had to be moved far from where it belongs loses to one that did not.
pub fn place_card(spot: Rect, tiles: &[Rect], card: Size, stage: Size) -> CardPlacement {
    let tiles: Vec<Rect> = tiles.iter().map(|tile| tile.shrunk(TILE_SHRINK)).collect();
    let (spot_x, spot_y) = spot.center();

    let mut best: Option<(f64, CardPlacement)> = None;
    for side in Side::ALL {
        // Along the side the card may slide, its arrow still reaching the spot.
        let reach = match side {
            Side::Top | Side::Bottom => card.width / 2.0 - ARROW_INSET,
            Side::Left | Side::Right => card.height / 2.0 - ARROW_INSET,
        };
        for slide in SLIDES {
            let (x, y) = origin(side, spot, card);
            let (x, y) = match side {
                Side::Top | Side::Bottom => (x + slide * reach, y),
                Side::Left | Side::Right => (x, y + slide * reach),
            };
            let left = x.min(stage.width - card.width - MARGIN_X).max(MARGIN_X);
            let top = y.min(stage.height - card.height - MARGIN_Y).max(MARGIN_Y);
            let rect = Rect::at(left, top, card.width, card.height);

            let hidden: f64 = tiles.iter().map(|tile| rect.overlap_area(*tile)).sum();
            let moved = (left - x).abs() + (top - y).abs();
            let slid = (slide * reach).abs();
            let cost = hidden + SPOT_PENALTY * rect.overlap_area(spot) + moved + slid;

            if best.as_ref().is_none_or(|(least, _)| cost < *least) {
                let along = |at: f64, size: f64| at.clamp(ARROW_INSET, size - ARROW_INSET);
                best = Some((
                    cost,
                    CardPlacement {
                        side,
                        left,
                        top,
                        arrow_x: along(spot_x - left, card.width),
                        arrow_y: along(spot_y - top, card.height),
                    },
                ));
            }
        }
    }
    best.expect("there are four sides").1
}

/// The card for the hex `at` as the camera sees the board: `placed` are the
/// board's tiles, `pose` where the camera looks and `stage` the size of the
/// stage it looks onto.
pub fn place_card_at(at: Position, placed: &[Position], pose: Pose, stage: Size) -> CardPlacement {
    let center = board_center(placed, HEX_SIZE);
    let on_stage =
        |position: Position| rect_to_stage(hex_rect(position, HEX_SIZE), pose, center, stage);
    let tiles: Vec<Rect> = placed.iter().map(|&position| on_stage(position)).collect();
    place_card(on_stage(at), &tiles, CARD, stage)
}

/// The card's size as the stylesheet needs it, for a card that is not yet
/// placed.
pub fn size_style() -> String {
    format!("--card-w: {CARD_WIDTH}px; --card-h: {CARD_HEIGHT}px;")
}

/// Whether some side of `spot` has room for the card as it belongs there:
/// wholly on the stage and clear of the spot.
pub fn has_room(spot: Rect, card: Size, stage: Size) -> bool {
    let inside = Rect::new(
        MARGIN_X,
        MARGIN_Y,
        stage.width - MARGIN_X,
        stage.height - MARGIN_Y,
    );
    Side::ALL.into_iter().any(|side| {
        let (x, y) = origin(side, spot, card);
        let rect = Rect::at(x, y, card.width, card.height);
        inside.contains(&rect) && rect.overlap_area(spot) == 0.0
    })
}

/// The camera once a two-way spot is picked, `spot` being its rectangle on
/// the board: as it is when the card has room beside the spot, else the
/// spot centred, at a zoom between [`MIN_SPOT_ZOOM`] (wide enough to tap) and
/// [`max_fit_zoom`] (a spot zoomed in far leaves no room for a card), so it
/// has.
pub fn make_room(spot: Rect, pose: Pose, center: (f64, f64), stage: Size) -> Pose {
    if has_room(rect_to_stage(spot, pose, center, stage), CARD, stage) {
        return pose;
    }
    let (x, y) = spot.center();
    Pose {
        zoom: pose.zoom.clamp(MIN_SPOT_ZOOM, max_fit_zoom(stage)),
        pan_x: x - center.0,
        pan_y: y - center.1,
    }
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Direction, Position};

    use super::*;
    use crate::state::game_state::GameState;
    use crate::state::test_games::seeded;
    use crate::svg::camera_math::{on_select, spot_width, MIN_TARGET};
    use crate::svg::geometry::{board_center, hex_rect, hexes_rect, HEX_SIZE};

    const PHONE: Size = Size {
        width: 390.0,
        height: 600.0,
    };

    /// A 60×70 spot with its centre at (`x`, `y`).
    fn spot_at(x: f64, y: f64) -> Rect {
        Rect::at(x - 30.0, y - 35.0, 60.0, 70.0)
    }

    #[test]
    fn the_card_goes_above_the_spot_when_nothing_is_in_the_way() {
        let placed = place_card(spot_at(195.0, 300.0), &[], CARD, PHONE);
        assert_eq!(placed.side, Side::Top);
        assert_eq!(placed.left, 195.0 - CARD_WIDTH / 2.0);
        assert_eq!(placed.top, 300.0 - 35.0 - GAP - CARD_HEIGHT);
        // The arrow points down at the spot's middle.
        assert_eq!(placed.arrow_x, CARD_WIDTH / 2.0);
    }

    #[test]
    fn the_card_drops_below_a_spot_near_the_top_of_the_stage() {
        let placed = place_card(spot_at(195.0, 60.0), &[], CARD, PHONE);
        assert_eq!(placed.side, Side::Bottom);
        assert_eq!(placed.top, 60.0 + 35.0 + GAP);
    }

    #[test]
    fn the_card_keeps_off_the_tiles_around_the_spot() {
        // A spot in the middle of the stage with a tile on each of its four
        // upper and side neighbours, so only below is free.
        let on_stage = |position: Position| {
            rect_to_stage(hex_rect(position, HEX_SIZE), Pose::HOME, (0.0, 0.0), PHONE)
        };
        let spot = on_stage(Position::CENTER);
        let tiles: Vec<Rect> = [
            Direction::TopLeft,
            Direction::TopRight,
            Direction::Left,
            Direction::Right,
        ]
        .map(|dir| on_stage(Position::CENTER.neighbor(dir)))
        .to_vec();

        let placed = place_card(spot, &tiles, CARD, PHONE);
        assert_eq!(placed.side, Side::Bottom);
        for tile in &tiles {
            assert_eq!(
                placed.rect(CARD).overlap_area(tile.shrunk(TILE_SHRINK)),
                0.0
            );
        }
    }

    #[test]
    fn the_card_slides_along_its_side_to_a_free_pocket() {
        // Tiles above and to the right and left of the spot's top, leaving only a gap
        // above-left: the card is still above, but moved over to the free side.
        let spot = spot_at(195.0, 300.0);
        let blocked = Rect::at(215.0, 120.0, 140.0, 140.0);
        let placed = place_card(spot, &[blocked], CARD, PHONE);
        assert_eq!(
            placed.rect(CARD).overlap_area(blocked.shrunk(TILE_SHRINK)),
            0.0
        );
        assert!(placed.arrow_x > 0.0 && placed.arrow_x < CARD_WIDTH);
    }

    #[test]
    fn the_card_stays_on_the_stage_and_off_the_spot_wherever_the_spot_is() {
        let inside = Rect::new(
            MARGIN_X,
            MARGIN_Y,
            PHONE.width - MARGIN_X,
            PHONE.height - MARGIN_Y,
        );
        for x in [40.0, 100.0, 195.0, 290.0, 350.0] {
            for y in [40.0, 120.0, 300.0, 480.0, 560.0] {
                let spot = spot_at(x, y);
                let placed = place_card(spot, &[], CARD, PHONE);
                let rect = placed.rect(CARD);
                assert!(inside.contains(&rect), "({x}, {y}) {placed:?}");
                assert_eq!(rect.overlap_area(spot), 0.0, "({x}, {y}) {placed:?}");
                assert!((ARROW_INSET..=CARD_WIDTH - ARROW_INSET).contains(&placed.arrow_x));
                assert!((ARROW_INSET..=CARD_HEIGHT - ARROW_INSET).contains(&placed.arrow_y));
            }
        }
    }

    #[test]
    fn a_spot_in_a_corner_gets_its_card_beside_it() {
        let placed = place_card(spot_at(30.0, 40.0), &[], CARD, PHONE);
        assert!(matches!(placed.side, Side::Bottom | Side::Right));
        assert_eq!(placed.rect(CARD).overlap_area(spot_at(30.0, 40.0)), 0.0);
    }

    #[test]
    fn the_card_at_a_hex_is_placed_from_the_cameras_view_of_the_board() {
        // One tile, the camera centred on it: its hex is in the middle of the stage.
        let at = Position::CENTER;
        let pose = Pose::HOME;
        let direct = {
            let spot = rect_to_stage(hex_rect(at, HEX_SIZE), pose, (0.0, 0.0), PHONE);
            place_card(spot, &[spot], CARD, PHONE)
        };
        assert_eq!(place_card_at(at, &[at], pose, PHONE), direct);
        // A pan moves the card with the spot.
        let panned = Pose {
            pan_x: -50.0,
            ..pose
        };
        let moved = place_card_at(at, &[at], panned, PHONE);
        assert_eq!(moved.left, direct.left + 50.0);
    }

    #[test]
    fn the_card_style_carries_its_place_and_size() {
        let placed = CardPlacement {
            side: Side::Top,
            left: 12.0,
            top: 34.56,
            arrow_x: 90.0,
            arrow_y: 50.0,
        };
        assert_eq!(
            placed.style(),
            "left: 12.0px; top: 34.6px; --arrow-x: 90.0px; --arrow-y: 50.0px; \
             --card-w: 180px; --card-h: 100px;"
        );
        assert_eq!(placed.side.css_class(), "side-top");
    }

    #[test]
    fn a_spot_with_room_beside_it_keeps_the_camera_and_one_without_is_centred() {
        let center = (0.0, 0.0);
        let board_spot = hex_rect(Position::CENTER, HEX_SIZE);
        let pose = Pose {
            zoom: 0.7,
            pan_x: 3.0,
            pan_y: -4.0,
        };
        assert_eq!(make_room(board_spot, pose, center, PHONE), pose);

        // Panned so the spot sits in the top-left corner of the stage.
        let cornered = Pose {
            zoom: 0.4,
            pan_x: 180.0 / 0.4,
            pan_y: 260.0 / 0.4,
        };
        let tight = Size::new(120.0, 100.0);
        let moved = make_room(board_spot, cornered, center, tight);
        assert_eq!(moved.zoom, MIN_SPOT_ZOOM);
        assert_eq!((moved.pan_x, moved.pan_y), (0.0, 0.0));
    }

    // ---- over real positions

    /// Every (spot, all the tile's spots) where a hand tile of `game` fits
    /// two ways: the places that need the card.
    fn two_way_spots(game: &GameState) -> Vec<(Position, Vec<Position>)> {
        let mut found = Vec::new();
        for info in game.hand_info() {
            let all: Vec<Position> = info.positions.keys().copied().collect();
            for (&spot, rotations) in &info.positions {
                if rotations.len() > 1 {
                    found.push((spot, all.clone()));
                }
            }
        }
        found
    }

    /// The card's cost on a stage, measured the way a player sees it:
    /// how much of the placed tiles it covers, whether any tile's middle
    /// is under it, whether it covers the spot or leaves the stage.
    #[derive(Default, Debug)]
    struct Sweep {
        spots: usize,
        covered_area: f64,
        covered_tiles: usize,
        covers_spot: usize,
        leaves_stage: usize,
        spot_too_small: usize,
    }

    /// Replay every two-way spot of many seeded games on `stage`: the
    /// camera as `on_select` and `make_room` leave it, the card as
    /// `place_card` puts it (`smart`), or above the spot, below if it does
    /// not fit (the simple card the smart one is compared with).
    fn sweep(stage: Size, smart: bool) -> Sweep {
        let inside = Rect::new(0.0, 0.0, stage.width, stage.height);
        let mut result = Sweep::default();
        for seed in 0..24 {
            let mut game = seeded(seed);
            for _ in 0..16 {
                if !game.board().is_empty() {
                    let positions: Vec<Position> =
                        game.board().iter().map(|m| m.position).collect();
                    let center = board_center(&positions, HEX_SIZE);
                    let placed = hexes_rect(&positions, HEX_SIZE);
                    for (at, spots) in two_way_spots(&game) {
                        let spots_rect = hexes_rect(&spots, HEX_SIZE).unwrap();
                        let pose = on_select(placed, spots_rect, Pose::HOME, center, stage);
                        let spot = hex_rect(at, HEX_SIZE);
                        let pose = make_room(spot, pose, center, stage);
                        let spot = rect_to_stage(spot, pose, center, stage);
                        let tiles: Vec<Rect> = positions
                            .iter()
                            .map(|&p| rect_to_stage(hex_rect(p, HEX_SIZE), pose, center, stage))
                            .collect();

                        let card = if smart {
                            place_card(spot, &tiles, CARD, stage).rect(CARD)
                        } else {
                            let above = origin(Side::Top, spot, CARD);
                            let (x, y) = if above.1 < MARGIN_Y {
                                origin(Side::Bottom, spot, CARD)
                            } else {
                                above
                            };
                            let x = x.min(stage.width - CARD_WIDTH - MARGIN_X).max(MARGIN_X);
                            Rect::at(x, y, CARD_WIDTH, CARD_HEIGHT)
                        };

                        result.spots += 1;
                        result.covers_spot += (card.overlap_area(spot) > 0.0) as usize;
                        result.leaves_stage += !inside.contains(&card) as usize;
                        result.spot_too_small +=
                            (spot_width(pose.zoom) < MIN_TARGET - 1e-9) as usize;
                        for tile in &tiles {
                            result.covered_area += card.overlap_area(*tile);
                            let (x, y) = tile.center();
                            let middle = Rect::new(x, y, x, y);
                            result.covered_tiles += (card.contains(&middle)) as usize;
                        }
                    }
                }
                game.skip_stuck_players();
                let Some(m) = game.legal_moves().first().copied() else {
                    break;
                };
                game.play_move(m.placement()).unwrap();
            }
        }
        result
    }

    /// The stages a phone offers: portrait with and without a tall
    /// browser bar, landscape; a tablet's; and a laptop's and a full HD
    /// window's, as the desktop layout leaves them beside its rail.
    const STAGES: [Size; 6] = [
        Size {
            width: 390.0,
            height: 600.0,
        },
        Size {
            width: 360.0,
            height: 400.0,
        },
        Size {
            width: 612.0,
            height: 338.0,
        },
        Size {
            width: 900.0,
            height: 640.0,
        },
        Size {
            width: 985.0,
            height: 474.0,
        },
        Size {
            width: 1477.0,
            height: 729.0,
        },
    ];

    #[test]
    fn the_card_never_covers_its_spot_or_leaves_the_stage_in_real_games() {
        for stage in STAGES {
            let smart = sweep(stage, true);
            assert!(smart.spots >= 200, "{stage:?}: only {} spots", smart.spots);
            assert_eq!(smart.covers_spot, 0, "{stage:?}: {smart:?}");
            assert_eq!(smart.leaves_stage, 0, "{stage:?}: {smart:?}");
            assert_eq!(smart.spot_too_small, 0, "{stage:?}: {smart:?}");
        }
    }

    #[test]
    fn the_smart_card_hides_far_less_than_the_card_above_the_spot() {
        for stage in STAGES {
            let smart = sweep(stage, true);
            let above = sweep(stage, false);
            assert_eq!(smart.spots, above.spots);
            // At least three times less of the tiles covered, on every stage…
            assert!(
                smart.covered_area * 3.0 <= above.covered_area,
                "{stage:?}: smart {smart:?} vs above {above:?}"
            );
            assert!(
                smart.covered_tiles * 3 <= above.covered_tiles,
                "{stage:?}: smart {smart:?} vs above {above:?}"
            );
            // …and on a stage with room (all but the smallest phone), a tile's
            // middle is under it in about one spot in seven or less.
            if stage != STAGES[1] {
                assert!(
                    smart.covered_tiles * 7 <= smart.spots,
                    "{stage:?}: smart hides a tile's middle {} times in {} spots",
                    smart.covered_tiles,
                    smart.spots
                );
            }
        }
    }
}
