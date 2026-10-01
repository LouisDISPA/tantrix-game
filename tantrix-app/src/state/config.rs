use tantrix_ai::SmartAi;
use tantrix_game::{Color, Player};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayerKind {
    Off,
    Human,
    Ai,
}

impl PlayerKind {
    pub fn label(self) -> &'static str {
        match self {
            PlayerKind::Off => "Off",
            PlayerKind::Human => "Player",
            PlayerKind::Ai => "AI",
        }
    }

    pub fn next(self) -> Self {
        match self {
            PlayerKind::Off => PlayerKind::Human,
            PlayerKind::Human => PlayerKind::Ai,
            PlayerKind::Ai => PlayerKind::Off,
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
            Some((_, PlayerKind::Ai)) => Player::Ai(Box::new(SmartAi)),
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
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Color, Player};

    use super::{GameConfig, PlayerKind};

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
                PlayerKind::Ai,
                PlayerKind::Off,
                PlayerKind::Human
            ]
        );
        // The other seats keep what they were.
        assert_eq!(kind_of(&config, Color::Red), PlayerKind::Human);
        assert_eq!(kind_of(&config, Color::Green), PlayerKind::Off);
        let labels: Vec<_> = [PlayerKind::Off, PlayerKind::Human, PlayerKind::Ai]
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
}
