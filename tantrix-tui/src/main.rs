use std::io;

use crossterm::event::{self, KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color as RColor, Style, Styled, Stylize};
use ratatui::symbols::Marker;
use ratatui::text::Line as TextLine;
use ratatui::widgets::canvas::{Canvas, Context, Line as CanvasLine};
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};
use ratatui::DefaultTerminal;
use ratatui_macros::line;
use tantrix_ai::RandomAi;
use tantrix_game::{Color, Direction, Game, Move, Player, Position, Rotation, Tile, TileMoveInfo};

// Hex geometry constants for Canvas rendering
const HEX_RADIUS: f64 = 10.0;
const HAND_HEX_RADIUS: f64 = 8.0;
const SQRT3: f64 = 1.732_050_808;
const HEX_HORIZ: f64 = SQRT3 * HEX_RADIUS; // horizontal center-to-center for Right neighbor
const HEX_VERT: f64 = 1.5 * HEX_RADIUS; // vertical center-to-center for diagonal neighbors

fn main() -> io::Result<()> {
    let game = Game::new(
        Player::Someone,
        Player::Ai(Box::new(RandomAi)),
        Player::None,
        Player::None,
    )
    .expect("TUI starts with two active players");
    let app = App::new(game);
    ratatui::run(|terminal| app.run(terminal))
}

// --- Hex geometry helpers ---

/// Convert axial hex coordinates to canvas float coordinates.
/// Canvas y-axis points up, matching the game's coordinate system.
fn axial_to_canvas(x: i8, y: i8) -> (f64, f64) {
    let cx = (x as f64 - y as f64 * 0.5) * HEX_HORIZ;
    let cy = y as f64 * HEX_VERT;
    (cx, cy)
}

/// Compute the 6 vertices of a pointy-top hexagon centered at (cx, cy).
/// Returns vertices in order: top, upper-right, lower-right, bottom, lower-left, upper-left.
fn hex_vertices(cx: f64, cy: f64, r: f64) -> [(f64, f64); 6] {
    let half_w = r * SQRT3 / 2.0;
    [
        (cx, cy + r),                // 0: top
        (cx + half_w, cy + r / 2.0), // 1: upper-right
        (cx + half_w, cy - r / 2.0), // 2: lower-right
        (cx, cy - r),                // 3: bottom
        (cx - half_w, cy - r / 2.0), // 4: lower-left
        (cx - half_w, cy + r / 2.0), // 5: upper-left
    ]
}

/// Map a Direction to the pair of vertex indices forming that edge.
/// Each edge connects two adjacent vertices of the hex.
fn direction_edge(dir: Direction) -> (usize, usize) {
    match dir {
        Direction::TopRight => (0, 1),
        Direction::Right => (1, 2),
        Direction::BottomRight => (2, 3),
        Direction::BottomLeft => (3, 4),
        Direction::Left => (4, 5),
        Direction::TopLeft => (5, 0),
    }
}

/// Compute canvas viewport bounds from terminal area and camera offset.
fn canvas_bounds(area: Rect, camera: (i16, i16)) -> ([f64; 2], [f64; 2]) {
    // Braille: 2 dots per cell horizontally, 4 dots per cell vertically
    let dots_w = area.width as f64 * 2.0;
    let dots_h = area.height as f64 * 4.0;
    // Camera is in character-cell units; convert to canvas (dot) units
    let cam_x = camera.0 as f64 * 2.0;
    let cam_y = camera.1 as f64 * -4.0; // screen y-down → canvas y-up
    (
        [cam_x - dots_w / 2.0, cam_x + dots_w / 2.0],
        [cam_y - dots_h / 2.0, cam_y + dots_h / 2.0],
    )
}

/// Angular separation between two hex directions (1=adjacent, 2=skip-one, 3=opposite).
fn edge_separation(d1: Direction, d2: Direction) -> usize {
    let diff = ((d2 as i8) - (d1 as i8)).rem_euclid(6) as usize;
    diff.min(6 - diff)
}

/// Find the 3 pairs of directions sharing the same color on a placed tile.
fn find_color_pairs(m: &Move) -> [(Direction, Direction, Color); 3] {
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

/// Draw a quadratic Bezier curve approximated with N line segments.
fn draw_quadratic_bezier(
    ctx: &mut Context<'_>,
    p0: (f64, f64),
    ctrl: (f64, f64),
    p2: (f64, f64),
    color: RColor,
    segments: usize,
) {
    let mut prev = p0;
    for i in 1..=segments {
        let t = i as f64 / segments as f64;
        let u = 1.0 - t;
        let x = u * u * p0.0 + 2.0 * u * t * ctrl.0 + t * t * p2.0;
        let y = u * u * p0.1 + 2.0 * u * t * ctrl.1 + t * t * p2.1;
        ctx.draw(&CanvasLine::new(prev.0, prev.1, x, y, color));
        prev = (x, y);
    }
}

/// Draw a colored connecting curve between two same-colored edges inside a hex.
fn draw_color_link(
    ctx: &mut Context<'_>,
    cx: f64,
    cy: f64,
    d1: Direction,
    d2: Direction,
    color: RColor,
    verts: &[(f64, f64); 6],
) {
    let (i1, j1) = direction_edge(d1);
    let m1 = (
        (verts[i1].0 + verts[j1].0) / 2.0,
        (verts[i1].1 + verts[j1].1) / 2.0,
    );
    let (i2, j2) = direction_edge(d2);
    let m2 = (
        (verts[i2].0 + verts[j2].0) / 2.0,
        (verts[i2].1 + verts[j2].1) / 2.0,
    );

    match edge_separation(d1, d2) {
        1 => {
            let shared = if i1 == i2 || i1 == j2 { i1 } else { j1 };
            let sv = verts[shared];
            let ctrl = (cx + 0.35 * (sv.0 - cx), cy + 0.35 * (sv.1 - cy));
            draw_quadratic_bezier(ctx, m1, ctrl, m2, color, 10);
        }
        2 => draw_quadratic_bezier(ctx, m1, (cx, cy), m2, color, 10),
        3 => ctx.draw(&CanvasLine::new(m1.0, m1.1, m2.0, m2.1, color)),
        _ => {}
    }
}

/// Draw a hex tile with colored interior lines connecting same-colored edge pairs.
fn draw_hex_tile(ctx: &mut Context<'_>, cx: f64, cy: f64, m: &Move, r: f64, outline: RColor) {
    let verts = hex_vertices(cx, cy, r);
    draw_hex_outline_verts(ctx, &verts, outline);
    for (d1, d2, game_color) in find_color_pairs(m) {
        draw_color_link(
            ctx,
            cx,
            cy,
            d1,
            d2,
            game_color_to_ratatui(game_color),
            &verts,
        );
    }
}

fn draw_hex_outline_verts(ctx: &mut Context<'_>, verts: &[(f64, f64); 6], color: RColor) {
    for i in 0..6 {
        let j = (i + 1) % 6;
        ctx.draw(&CanvasLine::new(
            verts[i].0, verts[i].1, verts[j].0, verts[j].1, color,
        ));
    }
}

fn draw_hex_outline(ctx: &mut Context<'_>, cx: f64, cy: f64, r: f64, color: RColor) {
    draw_hex_outline_verts(ctx, &hex_vertices(cx, cy, r), color);
}

// --- App ---

#[derive(Debug, Clone, Copy, PartialEq)]
enum TurnPhase {
    SelectTile,
    SelectPosition,
    SelectRotation,
    GameOver,
}

struct App {
    game: Game,
    phase: TurnPhase,
    tile_index: usize,
    position_index: usize,
    rotation_index: usize,
    hand_info: Vec<TileMoveInfo>,
    sorted_positions: Vec<Position>,
    sorted_rotations: Vec<Rotation>,
    camera: (i16, i16),
    running: bool,
    message: String,
}

impl App {
    fn new(game: Game) -> Self {
        Self {
            game,
            phase: TurnPhase::SelectTile,
            tile_index: 0,
            position_index: 0,
            rotation_index: 0,
            hand_info: Vec::new(),
            sorted_positions: Vec::new(),
            sorted_rotations: Vec::new(),
            camera: (0, 0),
            running: true,
            message: String::new(),
        }
    }

    fn run(mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        self.running = true;
        self.game.advance();
        self.refresh_hand_info();
        while self.running {
            terminal.draw(|frame| frame.render_widget(&self, frame.area()))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn handle_events(&mut self) -> io::Result<()> {
        if let Some(key) = event::read()?.as_key_press_event() {
            self.on_key_event(key);
        }
        Ok(())
    }

    fn on_key_event(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('w') => {
                self.camera.1 -= 2;
                return;
            }
            KeyCode::Char('s') => {
                self.camera.1 += 2;
                return;
            }
            KeyCode::Char('a') => {
                self.camera.0 -= 3;
                return;
            }
            KeyCode::Char('d') => {
                self.camera.0 += 3;
                return;
            }
            _ => {}
        }

        match self.phase {
            TurnPhase::SelectTile => self.handle_select_tile(key),
            TurnPhase::SelectPosition => self.handle_select_position(key),
            TurnPhase::SelectRotation => self.handle_select_rotation(key),
            TurnPhase::GameOver => self.handle_game_over(key),
        }
    }

    fn handle_select_tile(&mut self, key: KeyEvent) {
        let hand_len = self.current_hand().len();
        if hand_len == 0 {
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.running = false,
            KeyCode::Up if self.tile_index > 0 => {
                self.tile_index -= 1;
            }
            KeyCode::Down if self.tile_index + 1 < hand_len => {
                self.tile_index += 1;
            }
            KeyCode::Enter => {
                if self.tile_index < self.hand_info.len()
                    && self.hand_info[self.tile_index].has_moves()
                {
                    self.enter_select_position();
                } else {
                    self.message = "No valid moves for this tile.".to_string();
                }
            }
            _ => {}
        }
    }

    fn handle_select_position(&mut self, key: KeyEvent) {
        if self.sorted_positions.is_empty() {
            return;
        }
        match key.code {
            KeyCode::Esc => {
                self.phase = TurnPhase::SelectTile;
                self.sorted_positions.clear();
                self.sorted_rotations.clear();
                self.message = String::new();
            }
            KeyCode::Left => {
                if self.position_index > 0 {
                    self.position_index -= 1;
                } else {
                    self.position_index = self.sorted_positions.len() - 1;
                }
                self.update_rotations_for_position();
            }
            KeyCode::Right => {
                self.position_index = (self.position_index + 1) % self.sorted_positions.len();
                self.update_rotations_for_position();
            }
            KeyCode::Enter => {
                if self.sorted_rotations.len() == 1 {
                    self.play_selected_move();
                } else {
                    self.phase = TurnPhase::SelectRotation;
                    self.rotation_index = 0;
                    self.message = format!(
                        "{} rotation(s) at this position",
                        self.sorted_rotations.len()
                    );
                }
            }
            _ => {}
        }
    }

    fn handle_select_rotation(&mut self, key: KeyEvent) {
        if self.sorted_rotations.is_empty() {
            return;
        }
        match key.code {
            KeyCode::Esc => {
                self.phase = TurnPhase::SelectPosition;
                self.rotation_index = 0;
                self.message = String::new();
            }
            KeyCode::Left => {
                if self.rotation_index > 0 {
                    self.rotation_index -= 1;
                } else {
                    self.rotation_index = self.sorted_rotations.len() - 1;
                }
            }
            KeyCode::Right => {
                self.rotation_index = (self.rotation_index + 1) % self.sorted_rotations.len();
            }
            KeyCode::Enter => {
                self.play_selected_move();
            }
            _ => {}
        }
    }

    fn handle_game_over(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter) {
            self.running = false;
        }
    }

    fn enter_select_position(&mut self) {
        let info = &self.hand_info[self.tile_index];
        self.sorted_positions = info.positions.keys().copied().collect();
        self.position_index = 0;
        self.update_rotations_for_position();
        self.phase = TurnPhase::SelectPosition;
        self.message = format!("{} position(s) available", self.sorted_positions.len());
    }

    fn update_rotations_for_position(&mut self) {
        let info = &self.hand_info[self.tile_index];
        if let Some(pos) = self.sorted_positions.get(self.position_index) {
            self.sorted_rotations = info.positions.get(pos).cloned().unwrap_or_default();
        } else {
            self.sorted_rotations.clear();
        }
        self.rotation_index = 0;
    }

    fn play_selected_move(&mut self) {
        let tile = self.current_hand()[self.tile_index];
        let position = self.sorted_positions[self.position_index];
        let rotation = self.sorted_rotations[self.rotation_index];
        let forced = self.hand_info[self.tile_index].has_forced;

        let m = Move {
            tile,
            position,
            rotation,
            forced,
        };

        match self.game.play_move(m) {
            Ok(_) => {
                self.message = String::new();
                self.tile_index = 0;
                self.position_index = 0;
                self.rotation_index = 0;
                self.sorted_positions.clear();
                self.sorted_rotations.clear();
                self.after_move();
            }
            Err(e) => {
                self.message = format!("Error: {}", e);
            }
        }
    }

    fn after_move(&mut self) {
        self.game.advance();

        if self.game.is_game_over() {
            self.phase = TurnPhase::GameOver;
            self.message = "Game Over!".to_string();
            return;
        }

        self.refresh_hand_info();
        self.phase = TurnPhase::SelectTile;

        let has_forced = self.hand_info.iter().any(|info| info.has_forced);
        if has_forced {
            self.message = "Forced move! You must fill a forced space.".to_string();
        }
    }

    fn refresh_hand_info(&mut self) {
        self.hand_info = self.game.moves_by_tile();
    }

    fn current_hand(&self) -> &[Tile] {
        self.game.hand(self.game.current_player())
    }

    fn preview_move(&self) -> Option<Move> {
        let tile = *self.current_hand().get(self.tile_index)?;
        let position = *self.sorted_positions.get(self.position_index)?;
        let rotation = if self.phase == TurnPhase::SelectRotation {
            *self.sorted_rotations.get(self.rotation_index)?
        } else {
            *self.sorted_rotations.first()?
        };
        Some(Move {
            tile,
            position,
            rotation,
            forced: false,
        })
    }
}

// --- Rendering helpers ---

fn game_color_to_ratatui(color: Color) -> RColor {
    match color {
        Color::Red => RColor::Red,
        Color::Blue => RColor::Blue,
        Color::Yellow => RColor::Yellow,
        Color::Green => RColor::Green,
    }
}

fn rotation_count_label(n: usize) -> &'static str {
    match n {
        2 => "2R",
        3 => "3R",
        4 => "4R",
        5 => "5R",
        6 => "6R",
        _ => "",
    }
}

fn rotation_label(rot: Rotation) -> &'static str {
    match rot {
        Rotation::None => "0",
        Rotation::R60 => "60",
        Rotation::R120 => "120",
        Rotation::R180 => "180",
        Rotation::R240 => "240",
        Rotation::R300 => "300",
    }
}

// --- Board widget (Canvas-based) ---

struct BoardWidget<'a> {
    app: &'a App,
}

impl Widget for BoardWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (x_bounds, y_bounds) = canvas_bounds(area, self.app.camera);

        // Snapshot phase-dependent data before entering the paint closure
        let phase = self.app.phase;
        let tile_index = self.app.tile_index;
        let position_index = self.app.position_index;

        // Collect placed tiles
        let placed_moves: Vec<Move> = self.app.game.board().to_vec();
        let hand_info = &self.app.hand_info;
        let sorted_positions = &self.app.sorted_positions;
        let preview = self.app.preview_move();

        let overlay_positions: Vec<(f64, f64, usize, bool)> = match phase {
            TurnPhase::SelectTile if tile_index < hand_info.len() => {
                let info = &hand_info[tile_index];
                info.positions
                    .iter()
                    .map(|(&pos, rots)| {
                        let (cx, cy) = axial_to_canvas(pos.x(), pos.y());
                        (cx, cy, rots.len(), info.has_forced)
                    })
                    .collect()
            }
            TurnPhase::SelectPosition | TurnPhase::SelectRotation => {
                let info = &hand_info[tile_index];
                sorted_positions
                    .iter()
                    .map(|&pos| {
                        let (cx, cy) = axial_to_canvas(pos.x(), pos.y());
                        let rots = info.positions.get(&pos).map(|v| v.len()).unwrap_or(0);
                        (cx, cy, rots, info.has_forced)
                    })
                    .collect()
            }
            _ => Vec::new(),
        };

        let selected_canvas: Option<(f64, f64)> = match phase {
            TurnPhase::SelectPosition | TurnPhase::SelectRotation => sorted_positions
                .get(position_index)
                .map(|pos| axial_to_canvas(pos.x(), pos.y())),
            _ => None,
        };

        let canvas = Canvas::default()
            .x_bounds(x_bounds)
            .y_bounds(y_bounds)
            .marker(Marker::Braille)
            .paint(move |ctx| {
                for m in &placed_moves {
                    let (cx, cy) = axial_to_canvas(m.position.x(), m.position.y());
                    draw_hex_tile(ctx, cx, cy, m, HEX_RADIUS, RColor::DarkGray);
                }

                ctx.layer();

                for &(cx, cy, rots, forced) in &overlay_positions {
                    let is_selected = selected_canvas
                        .is_some_and(|(sx, sy)| (sx - cx).abs() < 0.1 && (sy - cy).abs() < 0.1);

                    if is_selected {
                        if let Some(ref pv) = preview {
                            draw_hex_tile(ctx, cx, cy, pv, HEX_RADIUS, RColor::White);
                        }
                        if rots > 1 {
                            let label = rotation_count_label(rots);
                            ctx.print(
                                cx,
                                cy - HEX_RADIUS - 2.0,
                                TextLine::from(label).style(Style::default().fg(RColor::LightCyan)),
                            );
                        }
                    } else {
                        let outline_color = if forced {
                            RColor::LightRed
                        } else if matches!(phase, TurnPhase::SelectPosition) {
                            RColor::Gray
                        } else {
                            RColor::DarkGray
                        };
                        draw_hex_outline(ctx, cx, cy, HEX_RADIUS, outline_color);

                        if rots > 1
                            && matches!(phase, TurnPhase::SelectTile | TurnPhase::SelectPosition)
                        {
                            let label = rotation_count_label(rots);
                            ctx.print(
                                cx,
                                cy,
                                TextLine::from(label).style(Style::default().fg(RColor::Gray)),
                            );
                        }
                    }
                }
            });

        canvas.render(area, buf);
    }
}

// --- Hand widget ---

struct HandWidget<'a> {
    app: &'a App,
    color: Color,
    is_current_player: bool,
}

impl Widget for HandWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let color_style = Style::default().fg(game_color_to_ratatui(self.color));
        let title = format!(" {} ", self.color);
        let active_marker = if self.is_current_player { " *" } else { "" };
        let full_title = format!("{}{}", title, active_marker);
        let block = Block::bordered().title(full_title.set_style(color_style));
        let inner = block.inner(area);
        block.render(area, buf);

        let hand = self.app.game.hand(self.color);
        if hand.is_empty() {
            Paragraph::new("(empty)")
                .style(Style::default().fg(RColor::DarkGray))
                .render(inner, buf);
            return;
        }

        let r = HAND_HEX_RADIUS;
        let dots_w = inner.width as f64 * 2.0;
        let dots_h = inner.height as f64 * 4.0;
        let tile_step = 2.0 * r + 2.0;
        let hex_cx = dots_w / 2.0;
        let first_cy = dots_h - r - 1.0;

        let any_forced =
            self.is_current_player && self.app.hand_info.iter().any(|info| info.has_forced);

        let hand_data: Vec<(Tile, bool, Option<RColor>)> = hand
            .iter()
            .enumerate()
            .map(|(i, tile)| {
                if self.is_current_player && i < self.app.hand_info.len() {
                    let info = &self.app.hand_info[i];
                    let selected =
                        self.app.phase == TurnPhase::SelectTile && i == self.app.tile_index;
                    let contour = if selected {
                        Some(RColor::White)
                    } else if info.has_forced {
                        Some(RColor::LightRed)
                    } else {
                        None
                    };
                    let draw_curves = info.has_moves() && (!any_forced || info.has_forced);
                    (*tile, draw_curves, contour)
                } else {
                    (*tile, true, None)
                }
            })
            .collect();

        let canvas = Canvas::default()
            .x_bounds([0.0, dots_w])
            .y_bounds([0.0, dots_h])
            .marker(Marker::Braille)
            .paint(move |ctx| {
                for (i, &(tile, draw_curves, contour)) in hand_data.iter().enumerate() {
                    let cx = hex_cx;
                    let cy = first_cy - i as f64 * tile_step;
                    if cy - r < 0.0 {
                        break;
                    }

                    let m = Move {
                        tile,
                        position: Position::CENTER,
                        rotation: Rotation::None,
                        forced: false,
                    };

                    if draw_curves {
                        draw_hex_tile(ctx, cx, cy, &m, r, RColor::DarkGray);
                    } else {
                        draw_hex_outline(ctx, cx, cy, r, RColor::DarkGray);
                    }

                    if let Some(color) = contour {
                        draw_hex_outline(ctx, cx, cy, r + 2.0, color);
                    }
                }
            });

        canvas.render(inner, buf);
    }
}

// --- Main app widget ---

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let vertical = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]);
        let [title_area, main_area] = area.layout(&vertical);

        // --- Title bar ---
        let current_color = self.game.current_player();
        let color_style = Style::default().fg(game_color_to_ratatui(current_color));
        let title = line![
            "Tantrix".light_blue().bold(),
            " | ",
            current_color.to_string().set_style(color_style).bold(),
            "'s turn",
            " | Bag: ",
            self.game.bag_count().to_string(),
            " | Board: ",
            self.game.board().len().to_string(),
            " tiles"
        ];
        Paragraph::new(title).centered().render(title_area, buf);

        // --- Main: left panel (hands + info) | board ---
        let colors = self.game.playing_colors();
        let left_width = colors.len() as u16 * 20;
        let horizontal = Layout::horizontal([Constraint::Length(left_width), Constraint::Fill(1)]);
        let [left_area, board_area] = main_area.layout(&horizontal);

        // Board
        let board_block = Block::bordered().title(" Board ");
        let board_inner = board_block.inner(board_area);
        board_block.render(board_area, buf);
        BoardWidget { app: self }.render(board_inner, buf);

        // Left panel: info on top, hands below (side by side)
        let left_layout = Layout::vertical([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(5),
            Constraint::Length(3),
        ]);
        let [phase_area, hands_area, help_area, msg_area] = left_area.layout(&left_layout);

        // Hands side by side
        let hand_constraints: Vec<Constraint> =
            colors.iter().map(|_| Constraint::Fill(1)).collect();
        let hand_areas = Layout::horizontal(hand_constraints).split(hands_area);

        for (i, &color) in colors.iter().enumerate() {
            let is_current = color == self.game.current_player();
            HandWidget {
                app: self,
                color,
                is_current_player: is_current,
            }
            .render(hand_areas[i], buf);
        }

        // Phase
        let phase_text = match self.phase {
            TurnPhase::SelectTile => {
                let forced_count = self.hand_info.iter().filter(|i| i.has_forced).count();
                if forced_count > 0 {
                    format!("Select tile ({} forced!)", forced_count)
                } else {
                    "Select a tile".to_string()
                }
            }
            TurnPhase::SelectPosition => {
                format!(
                    "Pos {}/{} ({}R here)",
                    self.position_index + 1,
                    self.sorted_positions.len(),
                    self.sorted_rotations.len(),
                )
            }
            TurnPhase::SelectRotation => {
                let rot = self.sorted_rotations.get(self.rotation_index);
                format!(
                    "Rot {}/{}: {}deg",
                    self.rotation_index + 1,
                    self.sorted_rotations.len(),
                    rot.map(|r| rotation_label(*r)).unwrap_or("?"),
                )
            }
            TurnPhase::GameOver => "Game Over!".to_string(),
        };
        Paragraph::new(phase_text)
            .block(Block::bordered())
            .render(phase_area, buf);

        let help = match self.phase {
            TurnPhase::SelectTile => "Up/Down Enter w/a/s/d q",
            TurnPhase::SelectPosition => "Left/Right Enter Esc w/a/s/d",
            TurnPhase::SelectRotation => "Left/Right Enter Esc w/a/s/d",
            TurnPhase::GameOver => "q: quit",
        };
        Paragraph::new(help)
            .wrap(Wrap::default())
            .block(Block::bordered().title(" Help "))
            .render(help_area, buf);

        if !self.message.is_empty() {
            Paragraph::new(self.message.as_str())
                .style(Style::default().fg(RColor::LightYellow))
                .wrap(Wrap::default())
                .block(Block::bordered())
                .render(msg_area, buf);
        }
    }
}
