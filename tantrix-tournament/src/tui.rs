//! The tournament's live dashboard (ratatui): leaderboard, score rates,
//! score trends and the latest games, with pause, stop and restart.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Bar, BarChart, Block, Gauge, List, ListItem, Paragraph, Row, Sparkline, Table, Widget,
};

use crate::runner::Runner;
use crate::tournament::{GameResult, Standings, Tournament};

const MAX_RECENT: usize = 30;

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
    standings: Standings,
    recent: VecDeque<GameResult>,
    games_total: u32,
    phase: Phase,
    start_time: Instant,
    elapsed_at_pause: Duration,
    /// Best first, worked out once per frame with new results.
    ranking: Vec<usize>,
    elo: Vec<f64>,
}

impl TournamentState {
    fn new(seed: u64, names: &[String], games_total: u32) -> Self {
        Self {
            seed,
            standings: Standings::new(names),
            recent: VecDeque::new(),
            games_total,
            phase: Phase::Running,
            start_time: Instant::now(),
            elapsed_at_pause: Duration::ZERO,
            ranking: (0..names.len()).collect(),
            elo: vec![1500.0; names.len()],
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
            self.standings.games as f64 / secs
        } else {
            0.0
        }
    }

    fn record(&mut self, result: GameResult) {
        self.standings.record(&result);
        self.recent.push_front(result);
        if self.recent.len() > MAX_RECENT {
            self.recent.pop_back();
        }
    }

    /// Rank the AIs once per frame rather than once per result.
    fn rank(&mut self) {
        self.elo = self.standings.elo();
        self.ranking = self.standings.ranking();
    }

    fn name(&self, ai: usize) -> &str {
        &self.standings.stats[ai].name
    }
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
        "Builder" => Color::LightYellow,
        "Strategist" => Color::Yellow,
        "Lookahead" => Color::LightRed,
        "MonteCarlo" => Color::Red,
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

    let mid_layout = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]);
    let [leaderboard_area, right_area] = mid_area.layout(&mid_layout);

    render_leaderboard(state, leaderboard_area, buf);

    let right_layout = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]);
    let [barchart_area, sparkline_area] = right_area.layout(&right_layout);

    render_score_rate_chart(state, barchart_area, buf);
    render_sparklines(state, sparkline_area, buf);

    let bot_layout = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]);
    let [recent_area, help_area] = bot_area.layout(&bot_layout);

    render_recent(state, recent_area, buf);
    render_help(state, help_area, buf);
}

fn render_header(state: &TournamentState, active_games: u32, area: Rect, buf: &mut Buffer) {
    let layout = Layout::vertical([Constraint::Length(2), Constraint::Length(3)]);
    let [title_area, gauge_area] = area.layout(&layout);

    let elapsed = state.elapsed();
    let mins = elapsed.as_secs() / 60;
    let secs = elapsed.as_secs() % 60;

    let (status_text, status_style) = match state.phase {
        Phase::Running => ("RUNNING", Style::default().fg(Color::Green).bold()),
        Phase::Paused => ("PAUSED", Style::default().fg(Color::Yellow).bold()),
        Phase::Stopped => ("STOPPED", Style::default().fg(Color::Red).bold()),
        Phase::Complete => ("COMPLETE", Style::default().fg(Color::Cyan).bold()),
    };

    let dim = Style::default().fg(Color::DarkGray);
    let title = Line::from(vec![
        Span::styled(
            "  TANTRIX AI TOURNAMENT  ",
            Style::default().fg(Color::LightBlue).bold(),
        ),
        Span::raw("  "),
        Span::styled(status_text, status_style),
        Span::raw("  "),
        Span::styled(format!("seed {}", state.seed), dim),
        Span::raw("  "),
        Span::styled(format!("{}:{:02}", mins, secs), dim),
        Span::raw("  "),
        Span::styled(format!("{:.1} games/s", state.games_per_sec()), dim),
        Span::raw("  "),
        Span::styled(format!("{} in-flight", active_games), dim),
    ]);
    Paragraph::new(title).centered().render(title_area, buf);

    let completed = state.standings.games;
    let ratio = if state.games_total > 0 {
        completed as f64 / state.games_total as f64
    } else {
        0.0
    };
    let gauge_label = format!(
        "{} / {} games ({:.1}%)",
        completed,
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
        "Rank", "AI Name", "Elo", "W", "L", "D", "Score", "Avg", "Margin", "ms/mv",
    ])
    .style(Style::default().fg(Color::White).bold())
    .bottom_margin(1);

    let rows: Vec<Row> = state
        .ranking
        .iter()
        .enumerate()
        .map(|(rank, &ai)| {
            let s = &state.standings.stats[ai];
            let played = s.record.games() > 0;
            let or_dash = |text: String| if played { text } else { "-".to_string() };
            Row::new([
                medal(rank).to_string(),
                s.name.clone(),
                or_dash(format!("{:.0}", state.elo[ai])),
                s.record.wins.to_string(),
                s.record.losses.to_string(),
                s.record.draws.to_string(),
                or_dash(format!("{:.1}%", s.record.rate() * 100.0)),
                or_dash(format!("{:.1}", s.avg_score())),
                or_dash(format!("{:+.1}", s.avg_margin())),
                or_dash(format!("{:.2}", s.ms_per_move())),
            ])
            .style(medal_style(rank))
        })
        .collect();

    let widths = [
        Constraint::Length(5),  // rank
        Constraint::Length(11), // name
        Constraint::Length(5),  // Elo
        Constraint::Length(6),  // W
        Constraint::Length(6),  // L
        Constraint::Length(5),  // D
        Constraint::Length(7),  // Score
        Constraint::Length(5),  // Avg
        Constraint::Length(7),  // Margin
        Constraint::Length(7),  // ms/move
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

fn render_score_rate_chart(state: &TournamentState, area: Rect, buf: &mut Buffer) {
    let bars: Vec<Bar> = state
        .ranking
        .iter()
        .map(|&ai| {
            let s = &state.standings.stats[ai];
            let pct = (s.record.rate() * 100.0) as u64;
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
                .title(" Score (wins + ½ draws) ")
                .title_style(Style::default().bold()),
        )
        .bar_width(5)
        .bar_gap(1)
        .render(area, buf);
}

fn render_sparklines(state: &TournamentState, area: Rect, buf: &mut Buffer) {
    let block = Block::bordered()
        .title(" Score Trends ")
        .title_style(Style::default().bold());
    let inner = block.inner(area);
    block.render(area, buf);

    // The top 3.
    let top_n = state.ranking.len().min(3);
    let constraints: Vec<Constraint> = (0..top_n).map(|_| Constraint::Length(3)).collect();
    let rows = Layout::vertical(constraints).split(inner);

    for (i, &ai) in state.ranking.iter().take(top_n).enumerate() {
        let stat = &state.standings.stats[ai];
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
            let (won, lost, drew) = (
                Style::default().fg(Color::Green),
                Style::default().fg(Color::Red),
                Style::default().fg(Color::Yellow),
            );
            let (red_style, blue_style) = match r.winner() {
                Some(w) if w == r.red => (won, lost),
                Some(_) => (lost, won),
                None => (drew, drew),
            };

            // Marks who had the first turn.
            let (first_red, first_blue) = if r.red_moved_first {
                ("•", " ")
            } else {
                (" ", "•")
            };
            let first_style = Style::default().fg(Color::DarkGray);
            ListItem::new(Line::from(vec![
                Span::styled(first_red, first_style),
                Span::styled(state.name(r.red).to_string(), red_style),
                Span::styled(
                    format!(" {} ", r.red_score),
                    Style::default().fg(Color::White).bold(),
                ),
                Span::styled("vs", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {} ", r.blue_score),
                    Style::default().fg(Color::White).bold(),
                ),
                Span::styled(state.name(r.blue).to_string(), blue_style),
                Span::styled(first_blue, first_style),
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
    let key = |k: &'static str, color: Color, what: &'static str| {
        Line::from(vec![
            Span::styled(k, Style::default().fg(color).bold()),
            Span::raw(what),
        ])
    };
    let controls = match state.phase {
        Phase::Running => vec![
            key(" [p] ", Color::Yellow, "Pause"),
            key(" [s] ", Color::Red, "Stop (finish running)"),
            key(" [q] ", Color::Red, "Quit immediately"),
        ],
        Phase::Paused => vec![
            key(" [r] ", Color::Green, "Resume"),
            key(" [s] ", Color::Red, "Stop (finish running)"),
            key(" [q] ", Color::Red, "Quit immediately"),
        ],
        Phase::Stopped | Phase::Complete => vec![
            key(" [n] ", Color::Green, "New tournament (next seed)"),
            key(" [q] ", Color::Red, "Quit"),
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

// --- Main loop ---

/// Run the dashboard over the tournament `new_tournament` makes from a
/// seed, starting from `seed`; [n] starts the next seed's.
pub fn run(
    seed: u64,
    threads: usize,
    new_tournament: impl Fn(u64) -> Tournament,
) -> std::io::Result<()> {
    ratatui::run(|terminal| {
        let mut ctx = TournamentCtx::new(new_tournament(seed), threads);

        loop {
            ctx.drain_results();

            if ctx.state.phase == Phase::Running
                && ctx.state.standings.games >= ctx.state.games_total
            {
                ctx.state.phase = Phase::Complete;
                ctx.state.elapsed_at_pause = ctx.state.elapsed();
            }

            terminal.draw(|frame| {
                let active = ctx.runner.in_flight();
                render(&ctx.state, active, frame.area(), frame.buffer_mut());
            })?;

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    match (key.code, ctx.state.phase) {
                        (KeyCode::Char('q') | KeyCode::Esc, _) => {
                            ctx.runner.stop();
                            return Ok(());
                        }
                        (KeyCode::Char('p'), Phase::Running) => ctx.pause(),
                        (KeyCode::Char('r'), Phase::Paused) => ctx.resume(),
                        (KeyCode::Char('s'), Phase::Running | Phase::Paused) => ctx.stop(),
                        (KeyCode::Char('n'), Phase::Stopped | Phase::Complete) => {
                            let next = ctx.state.seed.wrapping_add(1);
                            ctx = TournamentCtx::new(new_tournament(next), threads);
                        }
                        _ => {}
                    }
                }
            }
        }
    })
}

struct TournamentCtx {
    state: TournamentState,
    runner: Runner,
}

impl TournamentCtx {
    fn new(tournament: Tournament, threads: usize) -> Self {
        let state = TournamentState::new(
            tournament.seed,
            &tournament.names(),
            tournament.games() as u32,
        );
        Self {
            state,
            runner: Runner::start(tournament, threads),
        }
    }

    fn drain_results(&mut self) {
        let mut recorded = false;
        while let Ok(result) = self.runner.results.try_recv() {
            self.state.record(result);
            recorded = true;
        }
        if recorded {
            self.state.rank();
        }
    }

    fn pause(&mut self) {
        self.runner.pause();
        self.state.elapsed_at_pause = self.state.elapsed();
        self.state.phase = Phase::Paused;
    }

    fn resume(&mut self) {
        self.runner.resume();
        self.state.start_time = Instant::now();
        self.state.phase = Phase::Running;
    }

    fn stop(&mut self) {
        self.runner.stop();
        self.state.elapsed_at_pause = self.state.elapsed();
        self.state.phase = Phase::Stopped;
        self.drain_results();
    }
}
