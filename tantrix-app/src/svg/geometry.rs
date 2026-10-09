use tantrix_game::{Direction, Position};

pub const HEX_SIZE: f64 = 50.0;
pub const SQRT3: f64 = 1.732_050_808;

/// Hex radius used inside each tile's inline SVG viewBox.
pub const TILE_HEX: f64 = 30.0;
/// ViewBox dimensions: tight fit around the hex bounding box.
pub const TILE_VB_W: f64 = TILE_HEX * SQRT3;
pub const TILE_VB_H: f64 = TILE_HEX * 2.0;
/// Hex center within the viewBox.
pub const TILE_CX: f64 = TILE_VB_W / 2.0;
pub const TILE_CY: f64 = TILE_VB_H / 2.0;

/// Convert axial hex coordinates to pixel coordinates.
/// SVG y-axis points down, game y-axis points up (TopLeft = +y), so we negate y.
pub fn axial_to_pixel(pos: Position, hex_size: f64) -> (f64, f64) {
    let x = (pos.x() as f64 - pos.y() as f64 * 0.5) * SQRT3 * hex_size;
    let y = -(pos.y() as f64) * 1.5 * hex_size;
    (x, y)
}

/// Compute the 6 vertices of a pointy-top hexagon centered at (cx, cy).
/// Order: top, upper-right, lower-right, bottom, lower-left, upper-left.
/// In SVG coords: top = cy - size (up), bottom = cy + size (down).
pub fn hex_vertices(cx: f64, cy: f64, size: f64) -> [(f64, f64); 6] {
    let w = size * SQRT3 / 2.0;
    [
        (cx, cy - size),           // 0: top
        (cx + w, cy - size / 2.0), // 1: upper-right
        (cx + w, cy + size / 2.0), // 2: lower-right
        (cx, cy + size),           // 3: bottom
        (cx - w, cy + size / 2.0), // 4: lower-left
        (cx - w, cy - size / 2.0), // 5: upper-left
    ]
}

/// Map a Direction to the pair of vertex indices forming that edge.
/// In SVG coordinates (y inverted from TUI), the mapping is:
///   TopRight → edge between top (0) and upper-right (1)
///   Right → edge between upper-right (1) and lower-right (2)
///   BottomRight → edge between lower-right (2) and bottom (3)
///   BottomLeft → edge between bottom (3) and lower-left (4)
///   Left → edge between lower-left (4) and upper-left (5)
///   TopLeft → edge between upper-left (5) and top (0)
pub fn direction_edge(dir: Direction) -> (usize, usize) {
    match dir {
        Direction::TopRight => (0, 1),
        Direction::Right => (1, 2),
        Direction::BottomRight => (2, 3),
        Direction::BottomLeft => (3, 4),
        Direction::Left => (4, 5),
        Direction::TopLeft => (5, 0),
    }
}

/// Midpoint of an edge given the hex vertices and direction.
pub fn edge_midpoint(verts: &[(f64, f64); 6], dir: Direction) -> (f64, f64) {
    let (i, j) = direction_edge(dir);
    (
        (verts[i].0 + verts[j].0) / 2.0,
        (verts[i].1 + verts[j].1) / 2.0,
    )
}

/// Angular separation between two hex directions (1=adjacent, 2=skip-one, 3=opposite).
pub fn edge_separation(d1: Direction, d2: Direction) -> usize {
    let diff = ((d2 as i8) - (d1 as i8)).rem_euclid(6) as usize;
    diff.min(6 - diff)
}

/// CSS custom properties that anchor a hex tile div at its board position.
/// Paired with `.board-tile`/`.board-empty` CSS which reads `--hx/--hy/--hw/--hh`.
pub fn hex_position_style(position: Position, hex_size: f64) -> String {
    let (px, py) = axial_to_pixel(position, hex_size);
    let half_w = hex_size * SQRT3 / 2.0;
    let half_h = hex_size;
    format!(
        "--hx: {:.1}px; --hy: {:.1}px; --hw: {:.1}px; --hh: {:.1}px;",
        px - half_w,
        py - half_h,
        half_w * 2.0,
        half_h * 2.0
    )
}

/// Compute the bounding box of all placed tiles in pixel coordinates.
/// Returns (min_x, min_y, max_x, max_y) or None if no tiles.
pub fn board_bounding_box(positions: &[Position], hex_size: f64) -> Option<(f64, f64, f64, f64)> {
    hexes_rect(positions, hex_size).map(|r| (r.min_x, r.min_y, r.max_x, r.max_y))
}

/// The centre of the placed tiles' bounding box, or the origin on an empty
/// board: the point the board's layer is centred on.
pub fn board_center(positions: &[Position], hex_size: f64) -> (f64, f64) {
    hexes_rect(positions, hex_size).map_or((0.0, 0.0), |bounds| bounds.center())
}

/// An axis-aligned rectangle in pixels (y grows downwards).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Rect {
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    /// The rectangle `width` × `height` whose top-left corner is (`x`, `y`).
    pub fn at(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self::new(x, y, x + width, y + height)
    }

    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }

    pub fn center(&self) -> (f64, f64) {
        (
            (self.min_x + self.max_x) / 2.0,
            (self.min_y + self.max_y) / 2.0,
        )
    }

    /// The smallest rectangle holding both.
    pub fn union(self, other: Rect) -> Rect {
        Rect::new(
            self.min_x.min(other.min_x),
            self.min_y.min(other.min_y),
            self.max_x.max(other.max_x),
            self.max_y.max(other.max_y),
        )
    }

    /// Each side moved in by `fraction` of the rectangle's width (left and
    /// right) or height (top and bottom).
    pub fn shrunk(self, fraction: f64) -> Rect {
        let (dx, dy) = (self.width() * fraction, self.height() * fraction);
        Rect::new(
            self.min_x + dx,
            self.min_y + dy,
            self.max_x - dx,
            self.max_y - dy,
        )
    }

    /// The area the two share (zero when they only touch or are apart).
    pub fn overlap_area(self, other: Rect) -> f64 {
        let w = self.max_x.min(other.max_x) - self.min_x.max(other.min_x);
        let h = self.max_y.min(other.max_y) - self.min_y.max(other.min_y);
        w.max(0.0) * h.max(0.0)
    }

    /// Whether `other` lies wholly inside this rectangle.
    pub fn contains(&self, other: &Rect) -> bool {
        other.min_x >= self.min_x
            && other.max_x <= self.max_x
            && other.min_y >= self.min_y
            && other.max_y <= self.max_y
    }
}

/// The pixel box of the hex at `pos`: the tile's bounding rectangle.
pub fn hex_rect(pos: Position, hex_size: f64) -> Rect {
    let (px, py) = axial_to_pixel(pos, hex_size);
    let half_w = hex_size * SQRT3 / 2.0;
    Rect::new(px - half_w, py - hex_size, px + half_w, py + hex_size)
}

/// The bounding box of the hexes at `positions` in pixel coordinates, or
/// `None` for none.
pub fn hexes_rect(positions: &[Position], hex_size: f64) -> Option<Rect> {
    positions
        .iter()
        .map(|&pos| hex_rect(pos, hex_size))
        .reduce(Rect::union)
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Direction, Position};

    use super::{
        axial_to_pixel, board_bounding_box, board_center, direction_edge, edge_separation,
        hex_rect, hexes_rect, Rect, HEX_SIZE, SQRT3,
    };

    const DIRECTIONS: [Direction; 6] = [
        Direction::Left,
        Direction::TopLeft,
        Direction::TopRight,
        Direction::Right,
        Direction::BottomRight,
        Direction::BottomLeft,
    ];

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
    }

    #[test]
    fn the_centre_hex_sits_at_the_origin_and_up_is_up() {
        let size = HEX_SIZE;
        assert_eq!(axial_to_pixel(Position::CENTER, size), (0.0, 0.0));
        let right = Position::CENTER.neighbor(Direction::Right);
        assert!(close(axial_to_pixel(right, size), (SQRT3 * size, 0.0)));
        // The game's y axis points up, SVG's down.
        let top_right = Position::CENTER.neighbor(Direction::TopRight);
        let (x, y) = axial_to_pixel(top_right, size);
        assert!(x > 0.0 && y < 0.0, "({x}, {y})");
        // Every neighbor is one hex width away.
        for dir in DIRECTIONS {
            let (x, y) = axial_to_pixel(Position::CENTER.neighbor(dir), size);
            assert!(
                ((x * x + y * y).sqrt() - SQRT3 * size).abs() < 1e-6,
                "{dir:?}"
            );
        }
    }

    #[test]
    fn each_direction_is_its_own_edge_of_the_hex() {
        let mut edges: Vec<_> = DIRECTIONS.iter().map(|&d| direction_edge(d)).collect();
        for &(a, b) in &edges {
            assert_eq!(b, (a + 1) % 6);
        }
        edges.sort_unstable();
        edges.dedup();
        assert_eq!(edges.len(), 6);
    }

    #[test]
    fn edges_are_one_two_or_three_apart() {
        assert_eq!(edge_separation(Direction::Left, Direction::TopLeft), 1);
        assert_eq!(edge_separation(Direction::Left, Direction::BottomLeft), 1);
        assert_eq!(edge_separation(Direction::Left, Direction::TopRight), 2);
        assert_eq!(edge_separation(Direction::Left, Direction::Right), 3);
        for a in DIRECTIONS {
            for b in DIRECTIONS {
                assert_eq!(edge_separation(a, b), edge_separation(b, a));
            }
        }
    }

    #[test]
    fn the_bounding_box_hugs_the_tiles() {
        assert_eq!(hexes_rect(&[], HEX_SIZE), None);
        let half_w = SQRT3 * HEX_SIZE / 2.0;
        let one = hexes_rect(&[Position::CENTER], HEX_SIZE).unwrap();
        assert_eq!(one, hex_rect(Position::CENTER, HEX_SIZE));
        assert!(close((one.min_x, one.min_y), (-half_w, -HEX_SIZE)));
        assert!(close((one.max_x, one.max_y), (half_w, HEX_SIZE)));

        let right = Position::CENTER.neighbor(Direction::Right);
        let two = hexes_rect(&[right, Position::CENTER], HEX_SIZE).unwrap();
        assert!(close(
            (two.min_x, two.max_x),
            (-half_w, SQRT3 * HEX_SIZE + half_w)
        ));
        // The tuple form is the same box.
        assert_eq!(
            board_bounding_box(&[right, Position::CENTER], HEX_SIZE),
            Some((two.min_x, two.min_y, two.max_x, two.max_y))
        );
        assert_eq!(board_bounding_box(&[], HEX_SIZE), None);
    }

    #[test]
    fn the_board_is_centred_on_its_tiles() {
        assert_eq!(board_center(&[], HEX_SIZE), (0.0, 0.0));
        assert_eq!(board_center(&[Position::CENTER], HEX_SIZE), (0.0, 0.0));
        let right = Position::CENTER.neighbor(Direction::Right);
        let (x, y) = board_center(&[Position::CENTER, right], HEX_SIZE);
        assert!(close((x, y), (SQRT3 * HEX_SIZE / 2.0, 0.0)));
    }

    #[test]
    fn rectangles_overlap_by_the_area_they_share() {
        let a = Rect::at(0.0, 0.0, 10.0, 10.0);
        assert_eq!((a.width(), a.height()), (10.0, 10.0));
        assert_eq!(a.center(), (5.0, 5.0));
        assert_eq!(a.overlap_area(Rect::at(5.0, 5.0, 10.0, 10.0)), 25.0);
        // Touching edges and distant rectangles share nothing.
        assert_eq!(a.overlap_area(Rect::at(10.0, 0.0, 4.0, 4.0)), 0.0);
        assert_eq!(a.overlap_area(Rect::at(30.0, 30.0, 4.0, 4.0)), 0.0);
        assert_eq!(a.overlap_area(a), 100.0);

        let b = Rect::at(20.0, -5.0, 10.0, 10.0);
        assert_eq!(a.union(b), Rect::new(0.0, -5.0, 30.0, 10.0));
        assert!(a.union(b).contains(&a) && a.union(b).contains(&b));
        assert!(!a.contains(&b));
    }

    #[test]
    fn shrinking_moves_each_side_in_by_a_share_of_the_size() {
        let r = Rect::at(0.0, 0.0, 100.0, 50.0).shrunk(0.1);
        assert!(close((r.min_x, r.min_y), (10.0, 5.0)));
        assert!(close((r.max_x, r.max_y), (90.0, 45.0)));
    }
}
