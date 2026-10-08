//! The tournament without its TUI: a progress line now and then on stderr,
//! then the results as plain text on stdout. For scripts and LLMs.

use std::fmt::Write as _;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use crate::runner::Runner;
use crate::tournament::{Standings, Tournament};

/// Print progress at least this often, and at every tenth of the games.
const PROGRESS_EVERY: Duration = Duration::from_secs(30);

pub fn run(tournament: Tournament, threads: usize) {
    let names = tournament.names();
    let games = tournament.games();
    let summary = summary(&tournament);
    eprintln!("{summary}: {games} games on {threads} threads");

    let start = Instant::now();
    let mut runner = Runner::start(tournament, threads);
    let mut standings = Standings::new(&names);
    let mut last_report = start;
    let mut tenths_reported = 0;
    loop {
        match runner.results.recv_timeout(Duration::from_millis(200)) {
            Ok(result) => standings.record(&result),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let done = standings.games as usize;
        let tenths = done * 10 / games.max(1);
        if tenths > tenths_reported || last_report.elapsed() >= PROGRESS_EVERY {
            tenths_reported = tenths;
            last_report = Instant::now();
            eprintln!("{}", progress(done, games, start.elapsed()));
        }
    }
    runner.stop();
    print!("{}", report(&summary, &standings, start.elapsed()));
}

/// What the tournament plays, in a line.
pub fn summary(tournament: &Tournament) -> String {
    format!(
        "Tantrix AI tournament, seed {}: {} AIs, {} games per pairing{}",
        tournament.seed,
        tournament.ais.len(),
        tournament.games_per_pair,
        if tournament.mirrored {
            " (each deal played twice, seats swapped)"
        } else {
            ""
        }
    )
}

fn progress(done: usize, games: usize, elapsed: Duration) -> String {
    let secs = elapsed.as_secs_f64();
    let rate = done as f64 / secs.max(1e-9);
    let eta = if done > 0 {
        format!("ETA {}", duration((games - done) as f64 / rate))
    } else {
        String::new()
    };
    format!(
        "[{:>7}] {done:>7}/{games} games {:>3}%  {:>8.1} games/s  {eta}",
        duration(secs),
        done * 100 / games.max(1),
        rate,
    )
}

/// `secs` as `1h02m`, `3m05s` or `12.3s`.
fn duration(secs: f64) -> String {
    let whole = secs as u64;
    match whole {
        0..60 => format!("{secs:.1}s"),
        60..3600 => format!("{}m{:02}s", whole / 60, whole % 60),
        _ => format!("{}h{:02}m", whole / 3600, whole % 3600 / 60),
    }
}

/// The standings as plain text: the leaderboard, then every head-to-head.
pub fn report(summary: &str, standings: &Standings, elapsed: Duration) -> String {
    let mut out = String::new();
    let secs = elapsed.as_secs_f64();
    let _ = writeln!(
        out,
        "{summary}\n{} games in {} ({:.1} games/s)\n",
        standings.games,
        duration(secs),
        standings.games as f64 / secs.max(1e-9),
    );

    let elo = standings.elo();
    let ranking = standings.ranking();
    let width = standings
        .stats
        .iter()
        .map(|s| s.name.len())
        .max()
        .unwrap_or(2)
        .max(2);
    let _ = writeln!(
        out,
        "Rank  {:<width$}   Elo   Score  ±95%    Wins  Losses  Draws  Avg pts  Margin  ms/move",
        "AI"
    );
    for (rank, &ai) in ranking.iter().enumerate() {
        let s = &standings.stats[ai];
        let _ = writeln!(
            out,
            "{:>4}  {:<width$}  {:>4.0}  {:>5.1}%  {:>4.1}  {:>6}  {:>6}  {:>5}  {:>7.2}  {:>+6.2}  {:>7.3}",
            rank + 1,
            s.name,
            elo[ai],
            s.record.rate() * 100.0,
            s.record.margin_of_error() * 100.0,
            s.record.wins,
            s.record.losses,
            s.record.draws,
            s.avg_score(),
            s.avg_margin(),
            s.ms_per_move(),
        );
    }

    let _ = writeln!(
        out,
        "\nHead to head: the row AI's score against the column AI, in % (a draw counts half)"
    );
    let column = 7;
    let short = |name: &str| name.chars().take(column).collect::<String>();
    let _ = write!(out, "{:<width$}", "");
    for &ai in &ranking {
        let _ = write!(out, " {:>column$}", short(&standings.stats[ai].name));
    }
    let _ = writeln!(out);
    for &row in &ranking {
        let _ = write!(out, "{:<width$}", standings.stats[row].name);
        for &col in &ranking {
            let record = standings.head_to_head[row][col];
            if row == col || record.games() == 0 {
                let _ = write!(out, " {:>column$}", "-");
            } else {
                let _ = write!(out, " {:>column$.1}", record.rate() * 100.0);
            }
        }
        let _ = writeln!(out);
    }

    let first = standings.first_mover;
    let _ = writeln!(
        out,
        "\nThe first player to move scored {:.1}% (±{:.1}).",
        first.rate() * 100.0,
        first.margin_of_error() * 100.0
    );
    out
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{duration, progress, report};
    use crate::tournament::{GameResult, Standings};

    #[test]
    fn durations_read_at_a_glance() {
        assert_eq!(duration(12.34), "12.3s");
        assert_eq!(duration(185.0), "3m05s");
        assert_eq!(duration(3720.0), "1h02m");
    }

    #[test]
    fn progress_shows_the_share_done_and_what_is_left() {
        let line = progress(250, 1000, Duration::from_secs(10));
        assert!(line.contains(" 250/1000 games  25%"), "{line}");
        assert!(line.contains("25.0 games/s"), "{line}");
        assert!(line.contains("ETA 30.0s"), "{line}");
    }

    #[test]
    fn the_report_ranks_the_ais_and_shows_every_pairing() {
        let names = ["Weak".to_string(), "Strong".to_string()];
        let mut standings = Standings::new(&names);
        for game in 0..10 {
            standings.record(&GameResult {
                red: game % 2,
                blue: 1 - game % 2,
                red_score: if game % 2 == 1 { 12 } else { 5 },
                blue_score: if game % 2 == 1 { 6 } else { 9 },
                red_moved_first: game < 6,
                think: [Duration::from_millis(1); 2],
                moves: [20, 20],
            });
        }
        let text = report("Summary", &standings, Duration::from_secs(2));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Summary");
        assert_eq!(lines[1], "10 games in 2.0s (5.0 games/s)");
        // Strong won every game: first, with every point.
        assert!(lines[4].starts_with("   1  Strong"), "{text}");
        assert!(lines[4].contains("100.0%"), "{text}");
        assert!(lines[5].starts_with("   2  Weak"), "{text}");
        assert!(text.contains("Strong       -   100.0"), "{text}");
        assert!(text.contains("Weak       0.0       -"), "{text}");
        assert!(text.contains("first player to move scored 50.0%"), "{text}");
    }
}
