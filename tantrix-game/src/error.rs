//! Why a move or a game setup was refused.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveError {
    NoLegalMoves,
    TileNotInHand,
    InvalidMove,
    BoardFull,
}

impl fmt::Display for MoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoveError::NoLegalMoves => write!(f, "No legal moves available"),
            MoveError::TileNotInHand => write!(f, "Tile not in hand"),
            MoveError::InvalidMove => write!(f, "Invalid move"),
            MoveError::BoardFull => write!(f, "Board is full"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupError {
    NotEnoughPlayers,
    EmptyTurnCycle,
    CurrentPlayerNotInCycle,
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetupError::NotEnoughPlayers => write!(f, "A game needs at least two players"),
            SetupError::EmptyTurnCycle => write!(f, "Turn cycle cannot be empty"),
            SetupError::CurrentPlayerNotInCycle => write!(f, "Current player is not in turn cycle"),
        }
    }
}

/// Why a recorded [`TurnEvent`](crate::TurnEvent) could not be played again
/// ([`Game::replay_event`](crate::Game::replay_event)). The game is left as
/// it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventError {
    /// It is another player's turn, or another step of it.
    WrongTurn,
    /// The recorded move is not a legal move here.
    Move(MoveError),
    /// The move is legal but played out otherwise (another hand index,
    /// other tiles drawn): the event comes from another deal.
    Diverged,
    /// The retired color is not playing, or is the last one left.
    Retire(&'static str),
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventError::WrongTurn => write!(f, "Not this player's turn"),
            EventError::Move(e) => write!(f, "{e}"),
            EventError::Diverged => write!(f, "The move played out differently"),
            EventError::Retire(e) => write!(f, "{e}"),
        }
    }
}

/// Why [`Game::replay`](crate::Game::replay) could not rebuild a game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// The players cannot start a game.
    Setup(SetupError),
    /// The event at `index` could not be played again.
    Event { index: usize, error: EventError },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplayError::Setup(e) => write!(f, "{e}"),
            ReplayError::Event { index, error } => write!(f, "Event {index}: {error}"),
        }
    }
}
