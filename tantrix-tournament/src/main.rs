//! A round robin between Tantrix AIs: run with `--help`.

mod config;
mod headless;
mod runner;
mod tournament;
mod tui;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use tantrix_ai::all_ais;
use tantrix_game::TantrixAi;

use crate::config::Config;
use crate::tournament::Tournament;

const DEFAULT_GAMES_PER_PAIR: usize = 1000;

const HELP: &str = "\
Play every pairing of the Tantrix AIs against each other.

Usage: tantrix-tournament [OPTIONS] [SEED]

The same seed (0 by default) plays the same tournament, whatever the threads.

Options:
  -c, --config <FILE> The AIs to play, with their parameters, and the settings
                      below, from a TOML file (see configs/example.toml);
                      flags override its settings
      --seed <N>      The tournament seed (or give it bare, as SEED)
  -g, --games <N>     Games per pairing [default: 1000]
      --ais <A,B,..>  Only the AIs of these names (any case)
      --skip <A,B,..> Leave out the AIs of these names
      --no-mirror     Deal every game afresh, instead of playing each deal twice
                      with the AIs' seats swapped (--mirror: the default)
  -j, --threads <N>   Worker threads [default: one per core]
      --headless      No TUI: progress on stderr, then the results on stdout
                      (the default when stdout is not a terminal)
      --list          List the AIs and their parameters, and exit
  -h, --help          Show this help";

const PARAMETERS: &str = "\
Parameters in a config file's [[ai]] tables (see configs/example.toml):
  every AI    kind (one of the above), name
  Strategist  weights = { open_end, forced_end, ready_end, closable_loop,
  Builder                 joinable, opponent }
  Lookahead   weights, width (moves looked ahead from)
  MonteCarlo  weights, width (moves played out), playouts (per move, on
              average), halving (spend them by successive halving)";

/// What the command line asks for; `None` leaves it to the config file or
/// the default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Args {
    config: Option<PathBuf>,
    seed: Option<u64>,
    games_per_pair: Option<usize>,
    mirrored: Option<bool>,
    threads: Option<usize>,
    only: Option<Vec<String>>,
    skip: Vec<String>,
    headless: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Run(Args),
    List,
    Help,
}

fn parse(args: impl IntoIterator<Item = String>, terminal: bool) -> Result<Command, String> {
    let mut parsed = Args {
        headless: !terminal,
        ..Args::default()
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--list" => return Ok(Command::List),
            "--headless" | "--no-tui" => parsed.headless = true,
            "--mirror" => parsed.mirrored = Some(true),
            "--no-mirror" => parsed.mirrored = Some(false),
            "-c" | "--config" => parsed.config = Some(PathBuf::from(value("--config")?)),
            "--seed" => parsed.seed = Some(number(&value("--seed")?, "the seed")?),
            "-g" | "--games" => {
                parsed.games_per_pair = Some(at_least_one(&value("--games")?, "--games")?)
            }
            "-j" | "--threads" => {
                parsed.threads = Some(at_least_one(&value("--threads")?, "--threads")?)
            }
            "--ais" => parsed.only = Some(names(&value("--ais")?)),
            "--skip" => parsed.skip = names(&value("--skip")?),
            bare if !bare.starts_with('-') => parsed.seed = Some(number(bare, "the seed")?),
            unknown => return Err(format!("unknown option {unknown:?} (see --help)")),
        }
    }
    Ok(Command::Run(parsed))
}

fn number<T: std::str::FromStr>(text: &str, what: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{what} must be a number, not {text:?}"))
}

fn at_least_one(text: &str, what: &str) -> Result<usize, String> {
    match number(text, what)? {
        0 => Err(format!("{what} must be at least 1")),
        n => Ok(n),
    }
}

fn names(list: &str) -> Vec<String> {
    list.split(',')
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Everything a tournament needs, the command line over the config file
/// over the defaults.
struct Settings {
    seed: u64,
    games_per_pair: usize,
    mirrored: bool,
    threads: usize,
}

impl Settings {
    fn of(args: &Args, config: &Config) -> Result<Self, String> {
        let settings = Self {
            seed: args.seed.or(config.seed).unwrap_or(0),
            games_per_pair: args
                .games_per_pair
                .or(config.games)
                .unwrap_or(DEFAULT_GAMES_PER_PAIR),
            mirrored: args.mirrored.or(config.mirror).unwrap_or(true),
            threads: args
                .threads
                .or(config.threads)
                .unwrap_or_else(runner::default_threads),
        };
        if settings.games_per_pair == 0 || settings.threads == 0 {
            return Err("games and threads must be at least 1".to_string());
        }
        Ok(settings)
    }
}

/// The AIs of `config` (every AI if it lists none) that `args` keeps, in
/// order.
fn select_ais(args: &Args, config: &Config) -> Result<Vec<Box<dyn TantrixAi>>, String> {
    let ais = config.build_ais()?;
    let named = |ai: &dyn TantrixAi, list: &[String]| {
        list.iter().any(|name| ai.name().eq_ignore_ascii_case(name))
    };
    for name in args.only.iter().flatten().chain(&args.skip) {
        if !ais.iter().any(|ai| ai.name().eq_ignore_ascii_case(name)) {
            let list: Vec<&str> = ais.iter().map(|ai| ai.name()).collect();
            return Err(format!(
                "there is no AI {name:?}; there are {}",
                list.join(", ")
            ));
        }
    }
    let ais: Vec<_> = ais
        .into_iter()
        .filter(|ai| {
            args.only
                .as_ref()
                .is_none_or(|only| named(ai.as_ref(), only))
                && !named(ai.as_ref(), &args.skip)
        })
        .collect();
    let mut names: Vec<String> = ais.iter().map(|ai| ai.name().to_lowercase()).collect();
    names.sort_unstable();
    if let Some(pair) = names.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(format!(
            "two AIs are called {:?}: give them names of their own",
            pair[0]
        ));
    }
    if ais.len() < 2 {
        return Err("a tournament needs at least two AIs".to_string());
    }
    Ok(ais)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = match parse(std::env::args().skip(1), std::io::stdout().is_terminal())? {
        Command::Run(args) => args,
        Command::Help => {
            println!("{HELP}");
            return Ok(());
        }
        Command::List => {
            for ai in all_ais() {
                println!("{}", ai.name());
            }
            println!("\n{PARAMETERS}");
            return Ok(());
        }
    };
    let config = match &args.config {
        Some(path) => Config::load(path)?,
        None => Config::default(),
    };
    let settings = Settings::of(&args, &config)?;
    select_ais(&args, &config)?;
    let new_tournament = |seed| {
        let ais = select_ais(&args, &config).expect("the AIs were checked");
        Tournament::new(seed, ais, settings.games_per_pair, settings.mirrored)
    };

    if args.headless {
        headless::run(new_tournament(settings.seed), settings.threads);
        Ok(())
    } else {
        tui::run(settings.seed, settings.threads, new_tournament).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{parse, select_ais, Args, Command, Settings, DEFAULT_GAMES_PER_PAIR};
    use crate::config::Config;

    fn args(list: &[&str]) -> Args {
        match parse(list.iter().map(|a| a.to_string()), true) {
            Ok(Command::Run(args)) => args,
            other => panic!("{list:?} gave {other:?}"),
        }
    }

    #[test]
    fn a_bare_number_is_the_seed_and_the_rest_defaults() {
        let args = args(&["7"]);
        let settings = Settings::of(&args, &Config::default()).unwrap();
        assert_eq!(settings.seed, 7);
        assert_eq!(settings.games_per_pair, DEFAULT_GAMES_PER_PAIR);
        assert!(settings.mirrored && !args.headless);
        assert_eq!((args.only, args.skip), (None, vec![]));
    }

    #[test]
    fn flags_override_the_config_file() {
        let config = Config::parse("seed = 3\ngames = 40\nmirror = false\nthreads = 5").unwrap();
        let from_file = Settings::of(&args(&["-c", "t.toml"]), &config).unwrap();
        assert_eq!(
            (
                from_file.seed,
                from_file.games_per_pair,
                from_file.mirrored,
                from_file.threads
            ),
            (3, 40, false, 5)
        );

        let flags = args(&[
            "--config",
            "t.toml",
            "--seed",
            "9",
            "-g",
            "8",
            "--mirror",
            "-j",
            "2",
            "--headless",
        ]);
        assert_eq!(flags.config, Some(PathBuf::from("t.toml")));
        assert!(flags.headless);
        let settings = Settings::of(&flags, &config).unwrap();
        assert_eq!(
            (
                settings.seed,
                settings.games_per_pair,
                settings.mirrored,
                settings.threads
            ),
            (9, 8, true, 2)
        );
    }

    #[test]
    fn without_a_terminal_it_runs_headless() {
        let Ok(Command::Run(args)) = parse(Vec::new(), false) else {
            panic!("no arguments run the tournament");
        };
        assert!(args.headless);
    }

    #[test]
    fn bad_arguments_are_refused() {
        let parse = |list: &[&str]| parse(list.iter().map(|a| a.to_string()), true);
        assert!(parse(&["--games"]).is_err());
        assert!(parse(&["--games", "0"]).is_err());
        assert!(parse(&["--games", "many"]).is_err());
        assert!(parse(&["--threads", "0"]).is_err());
        assert!(parse(&["--fast"]).is_err());
        assert_eq!(parse(&["--list"]), Ok(Command::List));
        assert_eq!(parse(&["-h"]), Ok(Command::Help));
        let zero = Config::parse("games = 0").unwrap();
        assert!(Settings::of(&Args::default(), &zero).is_err());
    }

    #[test]
    fn ais_are_kept_and_skipped_by_name() {
        let names = |args: &Args, config: &Config| -> Vec<String> {
            select_ais(args, config)
                .unwrap()
                .iter()
                .map(|ai| ai.name().to_string())
                .collect()
        };
        let all = Config::default();
        assert_eq!(
            names(&args(&["--ais", "smart,GREEDY"]), &all),
            ["Greedy", "Smart"]
        );
        let skipped = names(&args(&["--skip", "random,firstmove"]), &all);
        assert!(!skipped.iter().any(|n| n == "Random" || n == "FirstMove"));
        assert!(skipped.iter().any(|n| n == "Greedy"));

        let config = Config::parse(
            "[[ai]]\nkind = \"Greedy\"\n[[ai]]\nkind = \"Random\"\n[[ai]]\nkind = \"Random\"\nname = \"Random 2\"",
        )
        .unwrap();
        assert_eq!(
            names(&Args::default(), &config),
            ["Greedy", "Random", "Random 2"]
        );
        assert_eq!(
            names(&args(&["--skip", "greedy"]), &config),
            ["Random", "Random 2"]
        );

        assert!(select_ais(&args(&["--ais", "greedy,nobody"]), &all).is_err());
        assert!(select_ais(&args(&["--ais", "greedy"]), &all).is_err());
        assert!(select_ais(&args(&["--skip", "nobody"]), &all).is_err());
        let twice = Config::parse("[[ai]]\nkind = \"Greedy\"\n[[ai]]\nkind = \"greedy\"").unwrap();
        assert!(select_ais(&Args::default(), &twice).is_err());
    }
}
