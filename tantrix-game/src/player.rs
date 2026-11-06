//! Who plays a color: a person, an AI, or nobody.

use std::fmt;

use enum_map::EnumMap;

use crate::{Board, Color, Hand, Move, TurnStep};

/// What an AI knows when it is its turn ([`Game::ai_view`](crate::Game::ai_view)):
/// the table (the board and every hand, all face up), whose turn it is and
/// at which step, and how many tiles are left in the bag, but not which
/// ones are drawn next. [`Self::determinize`] deals the unseen tiles into a
/// bag of the AI's own, to play the game on from here.
#[derive(Debug, Clone, Copy)]
pub struct AiView<'a> {
    /// The tiles placed so far.
    pub board: &'a Board,
    /// Every color's hand.
    pub hands: &'a EnumMap<Color, Hand>,
    /// The color the AI plays.
    pub current_player: Color,
    /// The colors in the game, in turn order.
    pub playing_colors: &'a [Color],
    /// Where the AI is within its turn.
    pub turn_step: TurnStep,
    /// How many tiles are left in the bag.
    pub bag_count: usize,
    /// The current player's legal moves; never empty.
    pub legal_moves: &'a [Move],
}

/// A computer player: picks one of the legal moves on its turn.
pub trait TantrixAi: Send + Sync {
    /// A short name for this AI, for display.
    fn name(&self) -> &str;
    /// A boxed copy of this AI.
    fn clone_box(&self) -> Box<dyn TantrixAi>;
    /// A copy of this AI whose random choices, if it makes any, follow
    /// `seed`, so a game it plays can be played again. AIs without
    /// randomness are copied as they are.
    fn with_seed(&self, _seed: u64) -> Box<dyn TantrixAi> {
        self.clone_box()
    }
    /// One of `view.legal_moves`.
    fn choose_move(&self, view: &AiView) -> Move;
}

impl Clone for Box<dyn TantrixAi> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Who plays a color.
pub enum Player {
    /// A person.
    Someone,
    /// A computer player.
    Ai(Box<dyn TantrixAi>),
    /// Nobody: the color is not in the game.
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
