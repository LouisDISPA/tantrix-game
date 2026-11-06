//! Why a move or a game setup was refused.

use std::fmt;

/// Why a move was refused ([`Game::play_move`](crate::Game::play_move),
/// [`Hand::play_tile`](crate::Hand::play_tile)). The game is left as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveError {
    /// The current player has no legal move.
    NoLegalMoves,
    /// The tile is not in the hand.
    TileNotInHand,
    /// The placement is not one of the legal moves.
    InvalidMove,
    /// The board refused the tile.
    Board(BoardError),
}

impl fmt::Display for MoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoveError::NoLegalMoves => write!(f, "No legal moves available"),
            MoveError::TileNotInHand => write!(f, "Tile not in hand"),
            MoveError::InvalidMove => write!(f, "Invalid move"),
            MoveError::Board(e) => write!(f, "{e}"),
        }
    }
}

/// Why [`Board::insert_move`](crate::Board::insert_move) refused a move.
/// The board only checks room for it, not the rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardError {
    /// A tile is already at the move's position.
    PositionTaken,
    /// The board holds a tile for every one in the game.
    Full,
}

impl fmt::Display for BoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoardError::PositionTaken => write!(f, "Position is taken"),
            BoardError::Full => write!(f, "Board is full"),
        }
    }
}

/// Why [`Game::retire_color`](crate::Game::retire_color) refused to retire
/// a color. The game is left as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetireError {
    /// The color is not in the turn cycle.
    NotPlaying,
    /// The color is the last one playing.
    LastPlayer,
}

impl fmt::Display for RetireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RetireError::NotPlaying => write!(f, "Color is not playing"),
            RetireError::LastPlayer => write!(f, "Cannot retire the last remaining player"),
        }
    }
}

/// Why a game or a turn order could not be set up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupError {
    /// Fewer than two colors are played.
    NotEnoughPlayers,
    /// The turn order has no colors.
    EmptyTurnCycle,
    /// The current player is not in the turn order.
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
    Retire(RetireError),
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
    Event {
        /// Where the event is in the history, from 0.
        index: usize,
        /// Why it could not be played again.
        error: EventError,
    },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplayError::Setup(e) => write!(f, "{e}"),
            ReplayError::Event { index, error } => write!(f, "Event {index}: {error}"),
        }
    }
}
