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
