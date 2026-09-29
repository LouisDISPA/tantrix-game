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
    if positions.is_empty() {
        return None;
    }

    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for &pos in positions {
        let (px, py) = axial_to_pixel(pos, hex_size);
        let half_w = hex_size * SQRT3 / 2.0;
        min_x = min_x.min(px - half_w);
        max_x = max_x.max(px + half_w);
        min_y = min_y.min(py - hex_size);
        max_y = max_y.max(py + hex_size);
    }

    Some((min_x, min_y, max_x, max_y))
}
