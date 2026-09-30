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
