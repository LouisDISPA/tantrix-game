use tantrix_ai::{HeuristicAi, LookaheadAi, MonteCarloAi, SmartAi};
use tantrix_game::{Color, Player, TantrixAi};

/// How well an AI seat plays: each level is a stronger AI than the last
/// (Elo from the tournament in `doc/ai.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Difficulty {
    /// `Smart`, 1457.
    Easy,
    /// `Strategist`, 1593.
    #[default]
    Medium,
    /// `Lookahead`, 1686.
    Hard,
    /// `MonteCarlo`, 2076.
    Expert,
}

impl Difficulty {
    pub const ALL: [Difficulty; 4] = [
        Difficulty::Easy,
        Difficulty::Medium,
        Difficulty::Hard,
        Difficulty::Expert,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Medium => "Medium",
            Difficulty::Hard => "Hard",
            Difficulty::Expert => "Expert",
        }
    }

    fn ai(self) -> Box<dyn TantrixAi> {
        match self {
            Difficulty::Easy => Box::new(SmartAi),
            Difficulty::Medium => Box::new(HeuristicAi::strategist()),
            Difficulty::Hard => Box::new(LookaheadAi::default()),
            Difficulty::Expert => Box::new(MonteCarloAi::default()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayerKind {
    Off,
    Human,
    Ai(Difficulty),
}

impl PlayerKind {
    pub fn label(self) -> &'static str {
        match self {
            PlayerKind::Off => "Off",
            PlayerKind::Human => "Player",
            PlayerKind::Ai(_) => "AI",
        }
    }

    pub fn next(self) -> Self {
        match self {
            PlayerKind::Off => PlayerKind::Human,
            PlayerKind::Human => PlayerKind::Ai(Difficulty::default()),
            PlayerKind::Ai(_) => PlayerKind::Off,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GameConfig {
    pub players: [(Color, PlayerKind); 4],
    /// Deals the same bag and turn order every time (`/game?seed=N`).
    pub seed: Option<u64>,
}

impl GameConfig {
    pub fn new(seed: Option<u64>) -> Self {
        GameConfig {
            players: [
                (Color::Red, PlayerKind::Human),
                (Color::Blue, PlayerKind::Human),
                (Color::Yellow, PlayerKind::Off),
                (Color::Green, PlayerKind::Off),
            ],
            seed,
        }
    }

    pub fn player_for(&self, color: Color) -> Player {
        match self.players.iter().find(|(c, _)| *c == color) {
            Some((_, PlayerKind::Human)) => Player::Someone,
            Some((_, PlayerKind::Ai(difficulty))) => Player::Ai(difficulty.ai()),
            _ => Player::None,
        }
    }

    pub fn active_count(&self) -> usize {
        self.players
            .iter()
            .filter(|(_, k)| *k != PlayerKind::Off)
            .count()
    }

    pub fn cycle_player(&mut self, color: Color) {
        if let Some((_, kind)) = self.players.iter_mut().find(|(c, _)| *c == color) {
            *kind = kind.next();
        }
    }

    /// Sets how well `color`'s AI seat plays.
    pub fn set_difficulty(&mut self, color: Color, difficulty: Difficulty) {
        if let Some((_, kind)) = self.players.iter_mut().find(|(c, _)| *c == color) {
            *kind = PlayerKind::Ai(difficulty);
        }
    }
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Color, Player};

    use super::{Difficulty, GameConfig, PlayerKind};

    const COLORS: [Color; 4] = [Color::Red, Color::Blue, Color::Yellow, Color::Green];

    fn kind_of(config: &GameConfig, color: Color) -> PlayerKind {
        config.players.iter().find(|(c, _)| *c == color).unwrap().1
    }

    #[test]
    fn a_new_game_seats_two_humans() {
        let config = GameConfig::new(Some(9));
        assert_eq!(config.seed, Some(9));
        assert_eq!(config.active_count(), 2);
        let kinds: Vec<_> = COLORS.iter().map(|&c| kind_of(&config, c)).collect();
        assert_eq!(
            kinds,
            [
                PlayerKind::Human,
                PlayerKind::Human,
                PlayerKind::Off,
                PlayerKind::Off
            ]
        );
        assert_eq!(GameConfig::new(None).seed, None);
    }

    #[test]
    fn a_seat_cycles_through_human_ai_and_off() {
        let mut config = GameConfig::new(None);
        let mut seen = Vec::new();
        for _ in 0..4 {
            config.cycle_player(Color::Yellow);
            seen.push(kind_of(&config, Color::Yellow));
        }
        assert_eq!(
            seen,
            [
                PlayerKind::Human,
                PlayerKind::Ai(Difficulty::Medium),
                PlayerKind::Off,
                PlayerKind::Human
            ]
        );
        // The other seats keep what they were.
        assert_eq!(kind_of(&config, Color::Red), PlayerKind::Human);
        assert_eq!(kind_of(&config, Color::Green), PlayerKind::Off);
        let labels: Vec<_> = [
            PlayerKind::Off,
            PlayerKind::Human,
            PlayerKind::Ai(Difficulty::Expert),
        ]
        .map(PlayerKind::label)
        .into();
        assert_eq!(labels, ["Off", "Player", "AI"]);
    }

    #[test]
    fn only_seats_in_play_count() {
        let mut config = GameConfig::new(None);
        config.cycle_player(Color::Green); // Off → Human
        config.cycle_player(Color::Yellow); // Off → Human
        config.cycle_player(Color::Yellow); // → AI
        assert_eq!(config.active_count(), 4);
        config.cycle_player(Color::Red); // Human → AI: still playing
        assert_eq!(config.active_count(), 4);
        config.cycle_player(Color::Red); // → Off
        assert_eq!(config.active_count(), 3);
    }

    #[test]
    fn each_seat_gets_the_player_it_is_set_to() {
        let mut config = GameConfig::new(None);
        config.cycle_player(Color::Blue); // Human → AI
        assert!(matches!(config.player_for(Color::Red), Player::Someone));
        assert!(matches!(config.player_for(Color::Blue), Player::Ai(_)));
        assert!(matches!(config.player_for(Color::Yellow), Player::None));
        assert!(matches!(config.player_for(Color::Green), Player::None));
    }

    #[test]
    fn each_difficulty_seats_its_own_ai() {
        let names: Vec<_> = Difficulty::ALL
            .iter()
            .map(|&difficulty| {
                let mut config = GameConfig::new(None);
                config.set_difficulty(Color::Blue, difficulty);
                match config.player_for(Color::Blue) {
                    Player::Ai(ai) => ai.name().to_string(),
                    other => panic!("{other:?}"),
                }
            })
            .collect();
        assert_eq!(names, ["Smart", "Strategist", "Lookahead", "MonteCarlo"]);
    }

    #[test]
    fn a_seat_set_to_ai_plays_medium_until_told_otherwise() {
        let mut config = GameConfig::new(None);
        config.cycle_player(Color::Blue); // Human → AI
        assert_eq!(
            kind_of(&config, Color::Blue),
            PlayerKind::Ai(Difficulty::Medium)
        );
        config.cycle_player(Color::Yellow); // Off → Human
        config.cycle_player(Color::Yellow); // → AI
        config.set_difficulty(Color::Blue, Difficulty::Hard);
        // Only that seat changes.
        assert_eq!(
            kind_of(&config, Color::Blue),
            PlayerKind::Ai(Difficulty::Hard)
        );
        assert_eq!(
            kind_of(&config, Color::Yellow),
            PlayerKind::Ai(Difficulty::Medium)
        );
    }
}
