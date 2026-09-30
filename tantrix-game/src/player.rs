//! Who plays a color: a person, an AI, or nobody.

use std::fmt;

use enum_map::EnumMap;

use crate::{Board, Color, Hand, Move};

pub trait TantrixAi: Send + Sync {
    fn name(&self) -> &str;
    fn clone_box(&self) -> Box<dyn TantrixAi>;
    fn choose_move(
        &self,
        board: &Board,
        hands: &EnumMap<Color, Hand>,
        current_player: Color,
        active_players: &[Color],
        possible_moves: &[Move],
    ) -> Move;
}

impl Clone for Box<dyn TantrixAi> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

pub enum Player {
    Someone,
    Ai(Box<dyn TantrixAi>),
    None,
}

impl Clone for Player {
    fn clone(&self) -> Self {
        match self {
            Player::Someone => Player::Someone,
            Player::Ai(ai) => Player::Ai(ai.clone_box()),
            Player::None => Player::None,
        }
    }
}

impl PartialEq for Player {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Player::Someone, Player::Someone)
                | (Player::Ai(_), Player::Ai(_))
                | (Player::None, Player::None)
        )
    }
}

impl Eq for Player {}

impl fmt::Debug for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Player::Someone => write!(f, "Someone"),
            Player::Ai(ai) => write!(f, "Ai({})", ai.name()),
            Player::None => write!(f, "None"),
        }
    }
}
