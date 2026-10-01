use tantrix_game::{Color, Direction, Rotation, Tile};

use super::geometry::{direction_edge, edge_midpoint, edge_separation, hex_vertices};

/// Generate SVG polygon points string for a hex at (cx, cy).
pub fn hex_polygon_points(cx: f64, cy: f64, size: f64) -> String {
    let verts = hex_vertices(cx, cy, size);
    verts
        .iter()
        .map(|(x, y)| format!("{:.1},{:.1}", x, y))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Find the 3 pairs of directions sharing the same color on `tile` turned by `rotation`.
pub fn find_color_pairs(tile: Tile, rotation: Rotation) -> [(Direction, Direction, Color); 3] {
    let dirs = [
        Direction::Left,
        Direction::TopLeft,
        Direction::TopRight,
        Direction::Right,
        Direction::BottomRight,
        Direction::BottomLeft,
    ];
    let mut pairs = [(Direction::Left, Direction::Left, Color::Red); 3];
    let mut count = 0;
    let mut used = [false; 6];

    for i in 0..6 {
        if used[i] {
            continue;
        }
        let c = tile.color(dirs[i] - rotation);
        for j in (i + 1)..6 {
            if !used[j] && tile.color(dirs[j] - rotation) == c {
                pairs[count] = (dirs[i], dirs[j], c);
                count += 1;
                used[i] = true;
                used[j] = true;
                break;
            }
        }
    }
    pairs
}

/// Generate an SVG path `d` attribute for a color link curve between two same-colored edges.
pub fn color_link_path(cx: f64, cy: f64, d1: Direction, d2: Direction, size: f64) -> String {
    let verts = hex_vertices(cx, cy, size);
    let m1 = edge_midpoint(&verts, d1);
    let m2 = edge_midpoint(&verts, d2);

    match edge_separation(d1, d2) {
        1 => {
            // Adjacent edges: circular arc centered on the shared vertex
            let (i1, j1) = direction_edge(d1);
            let (i2, j2) = direction_edge(d2);
            let shared = if i1 == i2 || i1 == j2 { i1 } else { j1 };
            let sv = verts[shared];

            // Radius = distance from shared vertex to edge midpoint
            let dx = m1.0 - sv.0;
            let dy = m1.1 - sv.1;
            let r = (dx * dx + dy * dy).sqrt();

            // Determine sweep direction so the arc curves toward the hex center
            // (away from the shared vertex)
            let cross = (m2.0 - m1.0) * (sv.1 - m1.1) - (m2.1 - m1.1) * (sv.0 - m1.0);
            let sweep = if cross > 0.0 { 1 } else { 0 };

            format!(
                "M {:.1} {:.1} A {:.1} {:.1} 0 0 {} {:.1} {:.1}",
                m1.0, m1.1, r, r, sweep, m2.0, m2.1
            )
        }
        2 => {
            // Skip-one edges: Bezier through center
            format!(
                "M {:.1} {:.1} Q {:.1} {:.1} {:.1} {:.1}",
                m1.0, m1.1, cx, cy, m2.0, m2.1
            )
        }
        3 => {
            // Opposite edges: straight line through center
            format!("M {:.1} {:.1} L {:.1} {:.1}", m1.0, m1.1, m2.0, m2.1)
        }
        _ => String::new(),
    }
}

/// Lowercase color name for CSS class composition.
pub fn color_name(color: Color) -> &'static str {
    match color {
        Color::Red => "red",
        Color::Blue => "blue",
        Color::Yellow => "yellow",
        Color::Green => "green",
    }
}

/// CSS class name for a game color.
pub fn color_css_class(color: Color) -> &'static str {
    match color {
        Color::Red => "hex-curve-red",
        Color::Blue => "hex-curve-blue",
        Color::Yellow => "hex-curve-yellow",
        Color::Green => "hex-curve-green",
    }
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Direction, Rotation};

    use super::{color_link_path, find_color_pairs};
    use crate::state::test_games::seeded;

    const ROTATIONS: [Rotation; 6] = [
        Rotation::None,
        Rotation::R60,
        Rotation::R120,
        Rotation::R180,
        Rotation::R240,
        Rotation::R300,
    ];

    #[test]
    fn a_tiles_three_curves_join_its_same_colored_edges() {
        // Every tile dealt to a seeded game, every way round.
        let game = seeded(3);
        let tiles = game
            .playing_colors()
            .iter()
            .flat_map(|&c| game.hand(c).to_vec());
        for tile in tiles {
            for rotation in ROTATIONS {
                let pairs = find_color_pairs(tile, rotation);
                let mut ends: Vec<i8> = Vec::new();
                for (a, b, color) in pairs {
                    assert_eq!(tile.color(a - rotation), color, "{tile:?} {rotation:?}");
                    assert_eq!(tile.color(b - rotation), color, "{tile:?} {rotation:?}");
                    ends.extend([i8::from(a), i8::from(b)]);
                }
                ends.sort_unstable();
                assert_eq!(ends, [0, 1, 2, 3, 4, 5], "{tile:?} {rotation:?}");
            }
        }
    }

    #[test]
    fn a_curve_bends_by_how_far_apart_its_edges_are() {
        let path = |a, b| color_link_path(0.0, 0.0, a, b, 30.0);
        assert!(path(Direction::Left, Direction::TopLeft).contains(" A "));
        assert!(path(Direction::Left, Direction::TopRight).contains(" Q "));
        assert!(path(Direction::Left, Direction::Right).contains(" L "));
        assert_eq!(path(Direction::Left, Direction::Left), "");
    }
}
