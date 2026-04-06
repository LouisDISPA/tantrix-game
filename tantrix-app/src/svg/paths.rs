use tantrix_game::{Color, Direction, Move};

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

/// Find the 3 pairs of directions sharing the same color on a tile (with rotation applied).
pub fn find_color_pairs(m: &Move) -> [(Direction, Direction, Color); 3] {
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
        let c = m.color(dirs[i]);
        for j in (i + 1)..6 {
            if !used[j] && m.color(dirs[j]) == c {
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
pub fn color_link_path(
    cx: f64,
    cy: f64,
    d1: Direction,
    d2: Direction,
    size: f64,
) -> String {
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

/// CSS class name for a game color.
pub fn color_css_class(color: Color) -> &'static str {
    match color {
        Color::Red => "hex-curve-red",
        Color::Blue => "hex-curve-blue",
        Color::Yellow => "hex-curve-yellow",
        Color::Green => "hex-curve-green",
    }
}

/// CSS color variable name for a game color.
pub fn color_css_var(color: Color) -> &'static str {
    match color {
        Color::Red => "var(--color-red)",
        Color::Blue => "var(--color-blue)",
        Color::Yellow => "var(--color-yellow)",
        Color::Green => "var(--color-green)",
    }
}

/// Lighter CSS color variable for a game color.
pub fn color_css_var_light(color: Color) -> &'static str {
    match color {
        Color::Red => "var(--color-red-light)",
        Color::Blue => "var(--color-blue-light)",
        Color::Yellow => "var(--color-yellow-light)",
        Color::Green => "var(--color-green-light)",
    }
}
