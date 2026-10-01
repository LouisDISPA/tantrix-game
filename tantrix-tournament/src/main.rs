use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode};
use rand::rngs::Xoshiro256PlusPlus;
use rand::{Rng, SeedableRng};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Bar, BarChart, Block, Gauge, List, ListItem, Paragraph, Row, Sparkline, Table, Widget,
};
use rayon::prelude::*;
use tantrix_ai::all_ais;
use tantrix_game::{Color as TileColor, Game, Player, TantrixAi};

const GAMES_PER_PAIR: usize = 10000;
const MAX_RECENT: usize = 30;
const SPARKLINE_WINDOW: usize = 60;

// --- Data types ---

#[derive(Debug, Clone, PartialEq, Eq)]
struct GameResult {
    name_a: String,
    name_b: String,
    /// `ai_a` plays Red and `ai_b` Blue: their scores are those colors'.
    score_a: usize,
    score_b: usize,
    /// The turn order is dealt at random: whether `ai_a` had the first turn.
    a_moved_first: bool,
}

impl GameResult {
    fn winner_name(&self) -> Option<&str> {
        match self.score_a.cmp(&self.score_b) {
            std::cmp::Ordering::Greater => Some(&self.name_a),
            std::cmp::Ordering::Less => Some(&self.name_b),
            std::cmp::Ordering::Equal => None,
        }
    }
}

#[derive(Default)]
struct AiStats {
    name: String,
    wins: u32,
    losses: u32,
    draws: u32,
    total_score: u64,
    games_played: u32,
    best_score: usize,
    worst_score: usize,
    total_margin: i64,
    recent_scores: VecDeque<u64>,
}

impl AiStats {
    fn win_rate(&self) -> f64 {
        if self.games_played > 0 {
            self.wins as f64 / self.games_played as f64
        } else {
            0.0
        }
    }

    fn avg_score(&self) -> u64 {
        if self.games_played > 0 {
            self.total_score / self.games_played as u64
        } else {
            0
        }
    }

    fn avg_margin(&self) -> f64 {
        if self.games_played > 0 {
            self.total_margin as f64 / self.games_played as f64
        } else {
            0.0
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Running,
    Paused,
    Stopped,
    Complete,
}

struct TournamentState {
    /// The same seed plays the same tournament.
    seed: u64,
    stats: Vec<AiStats>,
    recent: VecDeque<GameResult>,
    games_completed: u32,
    games_total: u32,
    phase: Phase,
    start_time: Instant,
    elapsed_at_pause: Duration,
}

impl TournamentState {
    fn new(seed: u64, ai_names: &[String], games_total: u32) -> Self {
        let stats = ai_names
            .iter()
            .map(|name| AiStats {
                name: name.clone(),
                worst_score: usize::MAX,
                ..Default::default()
            })
            .collect();
        Self {
            seed,
            stats,
            recent: VecDeque::new(),
            games_completed: 0,
            games_total,
            phase: Phase::Running,
            start_time: Instant::now(),
            elapsed_at_pause: Duration::ZERO,
        }
    }

    fn elapsed(&self) -> Duration {
        match self.phase {
            Phase::Running => self.elapsed_at_pause + self.start_time.elapsed(),
            _ => self.elapsed_at_pause,
        }
    }

    fn games_per_sec(&self) -> f64 {
        let secs = self.elapsed().as_secs_f64();
        if secs > 0.0 {
            self.games_completed as f64 / secs
        } else {
            0.0
        }
    }

    fn record(&mut self, result: GameResult) {
        self.games_completed += 1;

        for stat in &mut self.stats {
            let (is_a, is_b) = (stat.name == result.name_a, stat.name == result.name_b);
            if !is_a && !is_b {
                continue;
            }
            stat.games_played += 1;
            let my_score = if is_a { result.score_a } else { result.score_b };
            let opp_score = if is_a { result.score_b } else { result.score_a };
            stat.total_score += my_score as u64;
            stat.best_score = stat.best_score.max(my_score);
            if stat.worst_score == usize::MAX {
                stat.worst_score = my_score;
            } else {
                stat.worst_score = stat.worst_score.min(my_score);
            }
            stat.total_margin += my_score as i64 - opp_score as i64;

            stat.recent_scores.push_back(my_score as u64);
            if stat.recent_scores.len() > SPARKLINE_WINDOW {
                stat.recent_scores.pop_front();
            }

            match result.winner_name() {
                Some(w) if w == stat.name => stat.wins += 1,
                Some(_) => stat.losses += 1,
                None => stat.draws += 1,
            }
        }

        self.recent.push_front(result);
        if self.recent.len() > MAX_RECENT {
            self.recent.pop_back();
        }
    }

    /// Order the leaderboard by win rate, once per frame rather than once
    /// per result. The sort is stable: tied AIs keep their places.
    fn rank(&mut self) {
        self.stats
            .sort_by(|a, b| b.win_rate().partial_cmp(&a.win_rate()).unwrap());
    }
}

// --- Game runner ---

/// Play `ai_a` as Red against `ai_b` as Blue to the end. `seed` deals the
/// game and seeds both AIs, so a seed always plays the same game, whichever
/// thread plays it and when.
fn play_game(ai_a: &dyn TantrixAi, ai_b: &dyn TantrixAi, seed: u64) -> Game {
    let mut seeds = Xoshiro256PlusPlus::seed_from_u64(seed);
    let (seed_a, seed_b) = (seeds.next_u64(), seeds.next_u64());
    let mut game = Game::new_seeded(
        Player::Ai(ai_a.with_seed(seed_a)),
        Player::Ai(ai_b.with_seed(seed_b)),
        Player::None,
        Player::None,
        seed,
    )
    .expect("tournament games always have two AI players");
    game.advance();
    game
}

fn run_game(ai_a: &dyn TantrixAi, ai_b: &dyn TantrixAi, seed: u64) -> GameResult {
    result_of(ai_a, ai_b, &play_game(ai_a, ai_b, seed))
}

/// The result of `game`, [`play_game`]d by `ai_a` as Red and `ai_b` as Blue.
fn result_of(ai_a: &dyn TantrixAi, ai_b: &dyn TantrixAi, game: &Game) -> GameResult {
    let scores = game.scores();
    GameResult {
        name_a: ai_a.name().to_string(),
        name_b: ai_b.name().to_string(),
        score_a: scores[TileColor::Red].score,
        score_b: scores[TileColor::Blue].score,
        a_moved_first: game.playing_colors()[0] == TileColor::Red,
    }
}

/// The seed of every game of the tournament between `ai_count` AIs played
/// from `seed`, in game order (see [`pairing`]). Each game's own seed is
/// drawn here, in order, before any game runs: results don't depend on how
/// the games are scheduled. The games themselves are only set up as they
/// are played.
fn game_seeds(ai_count: usize, seed: u64) -> Vec<u64> {
    let mut seeds = Xoshiro256PlusPlus::seed_from_u64(seed);
    let games = ai_count * ai_count.saturating_sub(1) / 2 * GAMES_PER_PAIR;
    (0..games).map(|_| seeds.next_u64()).collect()
}

/// The AIs (indices among `ai_count`) that play game `game` of the
/// tournament, the first as Red: every pairing plays `GAMES_PER_PAIR`
/// games in a row, the first AI with each later one, then the second…
fn pairing(ai_count: usize, game: usize) -> (usize, usize) {
    let mut pair = game / GAMES_PER_PAIR;
    for a in 0..ai_count {
        let later = ai_count - a - 1;
        if pair < later {
            return (a, a + 1 + pair);
        }
        pair -= later;
    }
    panic!("the tournament has no game {game}")
}

// --- Color mapping ---

fn ai_color(name: &str) -> Color {
    match name {
        "FirstMove" => Color::DarkGray,
        "Random" => Color::Magenta,
        "Greedy" => Color::Green,
        "Defensive" => Color::Blue,
        "LoopHunter" => Color::Cyan,
        "Smart" => Color::LightGreen,
        _ => Color::White,
    }
}

fn medal(rank: usize) -> &'static str {
    match rank {
        0 => " 1st",
        1 => " 2nd",
        2 => " 3rd",
        _ => "    ",
    }
}

fn medal_style(rank: usize) -> Style {
    match rank {
        0 => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        1 => Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
        2 => Style::default().fg(Color::Cyan),
        _ => Style::default().fg(Color::DarkGray),
    }
}

// --- Rendering ---

fn render(state: &TournamentState, active_games: u32, area: Rect, buf: &mut Buffer) {
    let main_layout = Layout::vertical([
        Constraint::Length(5),  // title + progress
        Constraint::Min(12),    // middle section
        Constraint::Length(12), // bottom section
    ]);
    let [top_area, mid_area, bot_area] = area.layout(&main_layout);

    render_header(state, active_games, top_area, buf);

    let mid_layout = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]);
    let [leaderboard_area, right_area] = mid_area.layout(&mid_layout);

    render_leaderboard(state, leaderboard_area, buf);

    let right_layout = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]);
    let [barchart_area, sparkline_area] = right_area.layout(&right_layout);

    render_win_rate_chart(state, barchart_area, buf);
    render_sparklines(state, sparkline_area, buf);

    let bot_layout = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]);
    let [recent_area, help_area] = bot_area.layout(&bot_layout);

    render_recent(state, recent_area, buf);
    render_help(state, help_area, buf);
}

fn render_header(state: &TournamentState, active_games: u32, area: Rect, buf: &mut Buffer) {
    let layout = Layout::vertical([Constraint::Length(2), Constraint::Length(3)]);
    let [title_area, gauge_area] = area.layout(&layout);

    // Title bar
    let elapsed = state.elapsed();
    let mins = elapsed.as_secs() / 60;
    let secs = elapsed.as_secs() % 60;

    let (status_text, status_style) = match state.phase {
        Phase::Running => ("RUNNING", Style::default().fg(Color::Green).bold()),
        Phase::Paused => ("PAUSED", Style::default().fg(Color::Yellow).bold()),
        Phase::Stopped => ("STOPPED", Style::default().fg(Color::Red).bold()),
        Phase::Complete => ("COMPLETE", Style::default().fg(Color::Cyan).bold()),
    };

    let title = Line::from(vec![
        Span::styled(
            "  TANTRIX AI TOURNAMENT  ",
            Style::default().fg(Color::LightBlue).bold(),
        ),
        Span::raw("  "),
        Span::styled(status_text, status_style),
        Span::raw("  "),
        Span::styled(
            format!("seed {}", state.seed),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{}:{:02}", mins, secs),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{:.1} games/s", state.games_per_sec()),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} in-flight", active_games),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    Paragraph::new(title).centered().render(title_area, buf);

    // Progress gauge
    let ratio = if state.games_total > 0 {
        state.games_completed as f64 / state.games_total as f64
    } else {
        0.0
    };
    let gauge_label = format!(
        "{} / {} games ({:.1}%)",
        state.games_completed,
        state.games_total,
        ratio * 100.0,
    );
    let gauge_color = match state.phase {
        Phase::Running => Color::LightBlue,
        Phase::Paused => Color::Yellow,
        Phase::Stopped => Color::Red,
        Phase::Complete => Color::Green,
    };
    Gauge::default()
        .ratio(ratio)
        .label(gauge_label)
        .gauge_style(Style::default().fg(gauge_color))
        .block(Block::default())
        .render(gauge_area, buf);
}

fn render_leaderboard(state: &TournamentState, area: Rect, buf: &mut Buffer) {
    let header_row = Row::new([
        "Rank", "AI Name", "W", "L", "D", "Win%", "Avg", "Best", "Worst", "Margin",
    ])
    .style(Style::default().fg(Color::White).bold())
    .bottom_margin(1);

    let rows: Vec<Row> = state
        .stats
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let win_pct = if s.games_played > 0 {
                format!("{:.1}%", s.win_rate() * 100.0)
            } else {
                "-".to_string()
            };
            let avg = if s.games_played > 0 {
                format!("{}", s.avg_score())
            } else {
                "-".to_string()
            };
            let best = if s.games_played > 0 {
                s.best_score.to_string()
            } else {
                "-".to_string()
            };
            let worst = if s.games_played > 0 && s.worst_score != usize::MAX {
                s.worst_score.to_string()
            } else {
                "-".to_string()
            };
            let margin = if s.games_played > 0 {
                let m = s.avg_margin();
                if m >= 0.0 {
                    format!("+{:.1}", m)
                } else {
                    format!("{:.1}", m)
                }
            } else {
                "-".to_string()
            };

            let style = medal_style(i);

            Row::new([
                medal(i).to_string(),
                s.name.clone(),
                s.wins.to_string(),
                s.losses.to_string(),
                s.draws.to_string(),
                win_pct,
                avg,
                best,
                worst,
                margin,
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(5),  // rank
        Constraint::Length(11), // name
        Constraint::Length(5),  // W
        Constraint::Length(5),  // L
        Constraint::Length(4),  // D
        Constraint::Length(7),  // Win%
        Constraint::Length(5),  // Avg
        Constraint::Length(5),  // Best
        Constraint::Length(6),  // Worst
        Constraint::Length(7),  // Margin
    ];

    Table::new(rows, widths)
        .header(header_row)
        .block(
            Block::bordered()
                .title(" Leaderboard ")
                .title_style(Style::default().bold()),
        )
        .render(area, buf);
}

fn render_win_rate_chart(state: &TournamentState, area: Rect, buf: &mut Buffer) {
    let bars: Vec<Bar> = state
        .stats
        .iter()
        .map(|s| {
            let pct = (s.win_rate() * 100.0) as u64;
            Bar::default()
                .value(pct)
                .label(Line::from(s.name.as_str()).alignment(Alignment::Center))
                .style(Style::default().fg(ai_color(&s.name)))
                .text_value(format!("{}%", pct))
                .value_style(Style::default().fg(Color::White).bold())
        })
        .collect();

    BarChart::new(bars)
        .block(
            Block::bordered()
                .title(" Win Rate ")
                .title_style(Style::default().bold()),
        )
        .bar_width(7)
        .bar_gap(1)
        .render(area, buf);
}

fn render_sparklines(state: &TournamentState, area: Rect, buf: &mut Buffer) {
    let block = Block::bordered()
        .title(" Score Trends ")
        .title_style(Style::default().bold());
    let inner = block.inner(area);
    block.render(area, buf);

    if state.stats.is_empty() {
        return;
    }

    // Show top 3 sparklines
    let top_n = state.stats.len().min(3);
    let constraints: Vec<Constraint> = (0..top_n).map(|_| Constraint::Length(3)).collect();
    let rows = Layout::vertical(constraints).split(inner);

    for (i, stat) in state.stats.iter().take(top_n).enumerate() {
        if stat.recent_scores.is_empty() {
            continue;
        }
        let data: Vec<u64> = stat.recent_scores.iter().copied().collect();
        let label = format!(" {} ", stat.name);

        let spark_layout = Layout::horizontal([Constraint::Length(12), Constraint::Min(10)]);
        let [label_area, spark_area] = rows[i].layout(&spark_layout);

        Paragraph::new(Line::from(label).style(Style::default().fg(ai_color(&stat.name))))
            .render(label_area, buf);

        Sparkline::default()
            .data(&data)
            .style(Style::default().fg(ai_color(&stat.name)))
            .render(spark_area, buf);
    }
}

fn render_recent(state: &TournamentState, area: Rect, buf: &mut Buffer) {
    let items: Vec<ListItem> = state
        .recent
        .iter()
        .map(|r| {
            let (winner_style, loser_style) = match r.winner_name() {
                Some(_) => (
                    Style::default().fg(Color::Green),
                    Style::default().fg(Color::Red),
                ),
                None => (
                    Style::default().fg(Color::Yellow),
                    Style::default().fg(Color::Yellow),
                ),
            };

            let (a_style, b_style) = match r.winner_name() {
                Some(w) if w == r.name_a => (winner_style, loser_style),
                Some(_) => (loser_style, winner_style),
                None => (winner_style, loser_style),
            };

            // Marks who had the first turn.
            let (first_a, first_b) = if r.a_moved_first {
                ("•", " ")
            } else {
                (" ", "•")
            };
            let first_style = Style::default().fg(Color::DarkGray);
            ListItem::new(Line::from(vec![
                Span::styled(first_a, first_style),
                Span::styled(&r.name_a, a_style),
                Span::styled(
                    format!(" {} ", r.score_a),
                    Style::default().fg(Color::White).bold(),
                ),
                Span::styled("vs", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {} ", r.score_b),
                    Style::default().fg(Color::White).bold(),
                ),
                Span::styled(&r.name_b, b_style),
                Span::styled(first_b, first_style),
            ]))
        })
        .collect();

    List::new(items)
        .block(
            Block::bordered()
                .title(" Recent Games (• moved first) ")
                .title_style(Style::default().bold()),
        )
        .render(area, buf);
}

fn render_help(state: &TournamentState, area: Rect, buf: &mut Buffer) {
    let controls = match state.phase {
        Phase::Running => vec![
            Line::from(vec![
                Span::styled(" [p] ", Style::default().fg(Color::Yellow).bold()),
                Span::raw("Pause"),
            ]),
            Line::from(vec![
                Span::styled(" [s] ", Style::default().fg(Color::Red).bold()),
                Span::raw("Stop (finish running)"),
            ]),
            Line::from(vec![
                Span::styled(" [q] ", Style::default().fg(Color::Red).bold()),
                Span::raw("Quit immediately"),
            ]),
        ],
        Phase::Paused => vec![
            Line::from(vec![
                Span::styled(" [r] ", Style::default().fg(Color::Green).bold()),
                Span::raw("Resume"),
            ]),
            Line::from(vec![
                Span::styled(" [s] ", Style::default().fg(Color::Red).bold()),
                Span::raw("Stop (finish running)"),
            ]),
            Line::from(vec![
                Span::styled(" [q] ", Style::default().fg(Color::Red).bold()),
                Span::raw("Quit immediately"),
            ]),
        ],
        Phase::Stopped | Phase::Complete => vec![
            Line::from(vec![
                Span::styled(" [n] ", Style::default().fg(Color::Green).bold()),
                Span::raw("New tournament (next seed)"),
            ]),
            Line::from(vec![
                Span::styled(" [q] ", Style::default().fg(Color::Red).bold()),
                Span::raw("Quit"),
            ]),
        ],
    };

    Paragraph::new(controls)
        .block(
            Block::bordered()
                .title(" Controls ")
                .title_style(Style::default().bold()),
        )
        .render(area, buf);
}

// --- Main ---

/// `tantrix-tournament [SEED]`: the same seed (0 by default) plays the same
/// tournament.
fn main() -> std::io::Result<()> {
    let seed = match std::env::args().nth(1) {
        Some(arg) => arg.parse().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("the tournament seed must be a number, not {arg:?}"),
            )
        })?,
        None => 0,
    };
    ratatui::run(|terminal| {
        let mut ctx = TournamentCtx::new(seed);

        loop {
            ctx.drain_results();

            // Auto-complete
            if ctx.state.phase == Phase::Running
                && ctx.state.games_completed >= ctx.state.games_total
            {
                ctx.state.phase = Phase::Complete;
                ctx.state.elapsed_at_pause = ctx.state.elapsed();
            }

            terminal.draw(|frame| {
                let active = ctx.active_games.load(Ordering::Relaxed);
                render(&ctx.state, active, frame.area(), frame.buffer_mut());
            })?;

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    match (key.code, ctx.state.phase) {
                        (KeyCode::Char('q') | KeyCode::Esc, _) => {
                            ctx.cancel();
                            return Ok(());
                        }
                        (KeyCode::Char('p'), Phase::Running) => {
                            ctx.pause();
                        }
                        (KeyCode::Char('r'), Phase::Paused) => {
                            ctx.resume();
                        }
                        (KeyCode::Char('s'), Phase::Running | Phase::Paused) => {
                            ctx.stop();
                        }
                        (KeyCode::Char('n'), Phase::Stopped | Phase::Complete) => {
                            let next = ctx.state.seed.wrapping_add(1);
                            ctx = TournamentCtx::new(next);
                        }
                        _ => {}
                    }
                }
            }
        }
    })
}

// --- Tournament orchestrator ---

struct TournamentCtx {
    state: TournamentState,
    rx: Option<mpsc::Receiver<GameResult>>,
    cancel_flag: Arc<AtomicBool>,
    pause_flag: Arc<AtomicBool>,
    active_games: Arc<AtomicU32>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl TournamentCtx {
    fn new(seed: u64) -> Self {
        let ais = all_ais();
        let ai_names: Vec<String> = ais.iter().map(|a| a.name().to_string()).collect();
        let seeds = game_seeds(ais.len(), seed);
        let games_total = seeds.len() as u32;

        let (tx, rx) = mpsc::channel::<GameResult>();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let pause_flag = Arc::new(AtomicBool::new(false));
        let active_games = Arc::new(AtomicU32::new(0));

        let cancel = cancel_flag.clone();
        let pause = pause_flag.clone();
        let active = active_games.clone();

        let worker = std::thread::spawn(move || {
            seeds.par_iter().enumerate().for_each(|(game, &seed)| {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                // Spin-wait while paused
                while pause.load(Ordering::Relaxed) {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                active.fetch_add(1, Ordering::Relaxed);
                let (a, b) = pairing(ais.len(), game);
                let result = run_game(ais[a].as_ref(), ais[b].as_ref(), seed);
                active.fetch_sub(1, Ordering::Relaxed);
                let _ = tx.send(result);
            });
        });

        Self {
            state: TournamentState::new(seed, &ai_names, games_total),
            rx: Some(rx),
            cancel_flag,
            pause_flag,
            active_games,
            worker: Some(worker),
        }
    }

    fn drain_results(&mut self) {
        let Some(ref rx) = self.rx else {
            return;
        };
        let mut recorded = false;
        while let Ok(result) = rx.try_recv() {
            self.state.record(result);
            recorded = true;
        }
        if recorded {
            self.state.rank();
        }
    }

    fn pause(&mut self) {
        self.pause_flag.store(true, Ordering::Relaxed);
        self.state.elapsed_at_pause = self.state.elapsed();
        self.state.phase = Phase::Paused;
    }

    fn resume(&mut self) {
        self.pause_flag.store(false, Ordering::Relaxed);
        self.state.start_time = Instant::now();
        self.state.phase = Phase::Running;
    }

    fn stop(&mut self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
        self.state.elapsed_at_pause = self.state.elapsed();
        self.state.phase = Phase::Stopped;
        self.join_worker();
        self.drain_results();
    }

    fn cancel(&mut self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
        self.join_worker();
    }

    fn join_worker(&mut self) {
        if let Some(w) = self.worker.take() {
            w.join().ok();
        }
    }
}

impl Drop for TournamentCtx {
    fn drop(&mut self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
        self.join_worker();
    }
}

#[cfg(test)]
mod tests {
    use rayon::prelude::*;
    use tantrix_ai::{all_ais, FirstMoveAi, GreedyAi, RandomAi, SmartAi};
    use tantrix_game::Color;

    use super::{game_seeds, pairing, play_game, result_of, run_game, GameResult, GAMES_PER_PAIR};

    #[test]
    fn a_seed_plays_the_same_game_again() {
        let (random, smart) = (RandomAi::new(), SmartAi);
        for seed in 0..3 {
            assert_eq!(
                run_game(&random, &smart, seed),
                run_game(&random, &smart, seed)
            );
        }
    }

    #[test]
    fn each_ai_scores_the_color_it_plays() {
        let (first, greedy) = (FirstMoveAi, GreedyAi);
        let mut blue_first_with_other_scores = 0;
        for seed in 0..10 {
            let game = play_game(&first, &greedy, seed);
            let result = result_of(&first, &greedy, &game);

            // FirstMove is Red and Greedy Blue, whoever the deal lets move first.
            let scores = game.scores();
            assert_eq!(result.score_a, scores[Color::Red].score, "seed {seed}");
            assert_eq!(result.score_b, scores[Color::Blue].score, "seed {seed}");
            assert_eq!(result.a_moved_first, game.playing_colors()[0] == Color::Red);
            if !result.a_moved_first && result.score_a != result.score_b {
                blue_first_with_other_scores += 1;
            }
        }
        // Scoring by turn order instead of color would fail these games.
        assert!(blue_first_with_other_scores > 0);
    }

    #[test]
    fn every_pairing_plays_its_games_in_a_row() {
        let ai_count = all_ais().len();
        let games = game_seeds(ai_count, 0).len();
        let mut expected = Vec::new();
        for a in 0..ai_count {
            for b in (a + 1)..ai_count {
                expected.extend(std::iter::repeat_n((a, b), GAMES_PER_PAIR));
            }
        }
        assert_eq!(games, expected.len());
        let pairings: Vec<_> = (0..games).map(|game| pairing(ai_count, game)).collect();
        assert_eq!(pairings, expected);
    }

    #[test]
    fn a_tournament_does_not_depend_on_scheduling() {
        let ais = all_ais();
        let seeds = game_seeds(ais.len(), 0);
        assert_eq!(seeds, game_seeds(ais.len(), 0));
        assert_ne!(seeds, game_seeds(ais.len(), 1));

        // One game per pairing, played in order and in parallel.
        let sample: Vec<usize> = (0..seeds.len()).step_by(GAMES_PER_PAIR).collect();
        let play = |&game: &usize| {
            let (a, b) = pairing(ais.len(), game);
            run_game(ais[a].as_ref(), ais[b].as_ref(), seeds[game])
        };
        let in_order: Vec<GameResult> = sample.iter().map(play).collect();
        let in_parallel: Vec<GameResult> = sample.par_iter().map(play).collect();
        assert_eq!(in_order, in_parallel);
    }
}
