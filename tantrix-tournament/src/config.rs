//! Tournament config files (TOML): the AIs to play, with their parameters,
//! and the tournament's settings, which command-line flags override. See
//! `configs/example.toml`.

use std::path::Path;

use serde::Deserialize;
use tantrix_ai::{all_ais, HeuristicAi, LookaheadAi, MonteCarloAi, Weights};
use tantrix_game::{AiView, Move, TantrixAi};

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub seed: Option<u64>,
    /// Games per pairing.
    pub games: Option<usize>,
    /// Play each deal twice, the AIs' seats swapped.
    pub mirror: Option<bool>,
    pub threads: Option<usize>,
    /// The `[[ai]]` tables, in order: every AI of `all_ais` if there are none.
    #[serde(default, rename = "ai")]
    pub ais: Vec<AiConfig>,
}

/// One AI of the tournament.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    /// Which AI: a name `--list` shows, in any case.
    pub kind: String,
    /// What the tournament calls it: the kind's name by default.
    pub name: Option<String>,
    /// Strategist, Builder, Lookahead, MonteCarlo: the weights of their line
    /// evaluation; those left out keep the kind's.
    pub weights: Option<WeightsConfig>,
    /// Lookahead, MonteCarlo: how many of the best moves they search.
    pub width: Option<usize>,
    /// MonteCarlo: how many playouts per move, on average.
    pub playouts: Option<usize>,
    /// MonteCarlo: spend the playouts by successive halving (the default),
    /// or flat.
    pub halving: Option<bool>,
}

/// [`Weights`], each optional.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightsConfig {
    pub open_end: Option<f32>,
    pub forced_end: Option<f32>,
    pub ready_end: Option<f32>,
    pub closable_loop: Option<f32>,
    pub joinable: Option<f32>,
    pub opponent: Option<f32>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        Self::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|error| error.to_string())
    }

    /// The AIs the config lists, in order, or every AI if it lists none.
    pub fn build_ais(&self) -> Result<Vec<Box<dyn TantrixAi>>, String> {
        if self.ais.is_empty() {
            return Ok(all_ais());
        }
        self.ais.iter().map(AiConfig::build).collect()
    }
}

impl AiConfig {
    pub fn build(&self) -> Result<Box<dyn TantrixAi>, String> {
        let base = all_ais()
            .into_iter()
            .find(|ai| ai.name().eq_ignore_ascii_case(self.kind.trim()))
            .ok_or_else(|| {
                let names: Vec<String> = all_ais().iter().map(|ai| ai.name().to_string()).collect();
                format!(
                    "no AI is called {:?}; there are {}",
                    self.kind,
                    names.join(", ")
                )
            })?;
        let kind = base.name().to_string();
        let refuse = |param: &str, given: bool| {
            if given {
                Err(format!("{kind} takes no `{param}`"))
            } else {
                Ok(())
            }
        };
        let (weights, width, playouts, halving) = (
            self.weights.is_some(),
            self.width.is_some(),
            self.playouts.is_some(),
            self.halving.is_some(),
        );
        for (param, value) in [("width", self.width), ("playouts", self.playouts)] {
            if value == Some(0) {
                return Err(format!("{kind}'s `{param}` must be at least 1"));
            }
        }

        let ai: Box<dyn TantrixAi> = match kind.as_str() {
            "Strategist" | "Builder" => {
                refuse("width", width)?;
                refuse("playouts", playouts)?;
                refuse("halving", halving)?;
                let default = if kind == "Builder" {
                    Weights::SELFISH
                } else {
                    Weights::BALANCED
                };
                Box::new(HeuristicAi::new(kind.clone(), self.weights(default)))
            }
            "Lookahead" => {
                refuse("playouts", playouts)?;
                refuse("halving", halving)?;
                Box::new(LookaheadAi::new(
                    self.weights(Weights::BALANCED),
                    self.width.unwrap_or(LookaheadAi::DEFAULT_WIDTH),
                ))
            }
            "MonteCarlo" => Box::new(
                MonteCarloAi::new(
                    self.weights(Weights::BALANCED),
                    self.width.unwrap_or(MonteCarloAi::DEFAULT_WIDTH),
                    self.playouts.unwrap_or(MonteCarloAi::DEFAULT_PLAYOUTS),
                )
                .halving(self.halving.unwrap_or(true)),
            ),
            _ => {
                refuse("weights", weights)?;
                refuse("width", width)?;
                refuse("playouts", playouts)?;
                refuse("halving", halving)?;
                base
            }
        };
        Ok(match &self.name {
            Some(name) if name != ai.name() => Box::new(Renamed {
                name: name.clone(),
                ai,
            }),
            _ => ai,
        })
    }

    /// `default`, with the weights this config sets.
    fn weights(&self, default: Weights) -> Weights {
        let Some(set) = &self.weights else {
            return default;
        };
        Weights {
            open_end: set.open_end.unwrap_or(default.open_end),
            forced_end: set.forced_end.unwrap_or(default.forced_end),
            ready_end: set.ready_end.unwrap_or(default.ready_end),
            closable_loop: set.closable_loop.unwrap_or(default.closable_loop),
            joinable: set.joinable.unwrap_or(default.joinable),
            opponent: set.opponent.unwrap_or(default.opponent),
        }
    }
}

/// An AI playing under another name.
struct Renamed {
    name: String,
    ai: Box<dyn TantrixAi>,
}

impl TantrixAi for Renamed {
    fn name(&self) -> &str {
        &self.name
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(Renamed {
            name: self.name.clone(),
            ai: self.ai.clone_box(),
        })
    }

    fn with_seed(&self, seed: u64) -> Box<dyn TantrixAi> {
        Box::new(Renamed {
            name: self.name.clone(),
            ai: self.ai.with_seed(seed),
        })
    }

    fn choose_move(&self, view: &AiView) -> Move {
        self.ai.choose_move(view)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tantrix_ai::all_ais;

    use super::Config;

    fn names(text: &str) -> Result<Vec<String>, String> {
        let ais = Config::parse(text)?.build_ais()?;
        Ok(ais.iter().map(|ai| ai.name().to_string()).collect())
    }

    #[test]
    fn a_config_sets_the_tournament_and_its_ais_in_order() {
        let config = Config::parse(
            r#"
            seed = 4
            games = 20
            mirror = false
            threads = 3

            [[ai]]
            kind = "smart"

            [[ai]]
            kind = "MonteCarlo"
            name = "Wide MC"
            width = 10
            playouts = 4
            halving = true
            weights = { opponent = 0.5 }

            [[ai]]
            kind = "Random"
            name = "Random 2"
            "#,
        )
        .unwrap();
        assert_eq!(
            (config.seed, config.games, config.mirror, config.threads),
            (Some(4), Some(20), Some(false), Some(3))
        );
        let ais = config.build_ais().unwrap();
        let names: Vec<&str> = ais.iter().map(|ai| ai.name()).collect();
        assert_eq!(names, ["Smart", "Wide MC", "Random 2"]);
        // A renamed AI keeps its own seeded choices.
        assert_eq!(ais[2].with_seed(1).name(), "Random 2");
    }

    #[test]
    fn a_config_without_ais_plays_them_all() {
        let all: Vec<String> = all_ais().iter().map(|ai| ai.name().to_string()).collect();
        assert_eq!(names("games = 3").unwrap(), all);
    }

    #[test]
    fn mistakes_are_refused() {
        for text in [
            "gmaes = 3",
            "[[ai]]\nkind = \"Nobody\"",
            "[[ai]]\nkind = \"Greedy\"\nwidth = 2",
            "[[ai]]\nkind = \"Greedy\"\nweights = { opponent = 1 }",
            "[[ai]]\nkind = \"Strategist\"\nplayouts = 2",
            "[[ai]]\nkind = \"Lookahead\"\nplayouts = 2",
            "[[ai]]\nkind = \"Lookahead\"\nhalving = true",
            "[[ai]]\nkind = \"MonteCarlo\"\nwidth = 0",
            "[[ai]]\nkind = \"Strategist\"\nweights = { sideways = 1 }",
            "[[ai]]\nname = \"No kind\"",
            "games = \"many\"",
        ] {
            assert!(names(text).is_err(), "{text}");
        }
    }

    #[test]
    fn the_shipped_configs_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("configs");
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let config = Config::load(&path).unwrap();
            let ais = config.build_ais().unwrap();
            assert!(ais.len() >= 2, "{}", path.display());
            checked += 1;
        }
        assert!(checked > 0);
    }
}
