use dioxus::prelude::*;
use tantrix_game::{Color, Player};

#[derive(Debug, Clone, PartialEq)]
pub struct GameConfig {
    pub players: Vec<(Color, Player)>,
}

impl GameConfig {
    pub fn new(player_count: usize) -> Self {
        let colors = [Color::Red, Color::Blue, Color::Yellow, Color::Green];
        let players = colors
            .iter()
            .enumerate()
            .map(|(i, &color)| {
                if i < player_count {
                    (color, Player::Someone)
                } else {
                    (color, Player::None)
                }
            })
            .collect();
        GameConfig { players }
    }

    pub fn player_for(&self, color: Color) -> Player {
        self.players
            .iter()
            .find(|(c, _)| *c == color)
            .map(|(_, p)| *p)
            .unwrap_or(Player::None)
    }

    pub fn active_count(&self) -> usize {
        self.players.iter().filter(|(_, p)| *p != Player::None).count()
    }
}

pub static GAME_CONFIG: GlobalSignal<Option<GameConfig>> = Signal::global(|| None);
