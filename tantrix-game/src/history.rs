//! What happened in a game, in order. The board is sorted by position and
//! the bag only shrinks, so [`Game`](crate::Game) records every change to
//! the turn as a [`TurnEvent`]: enough to undo it exactly
//! ([`Game::undo`](crate::Game::undo)) or to play it again
//! ([`Game::replay`](crate::Game::replay)).

use serde::{Deserialize, Serialize};

use crate::{Color, DrawnTiles, Move, TurnStep};

/// One recorded change to a game, with the turn state before it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnEvent {
    /// `player`, at `step` of their turn, placed `m`.
    Played {
        player: Color,
        step: TurnStep,
        /// The move as placed, with the board's `forced` flag.
        m: Move,
        /// Where the tile was in the hand; the drawn tiles went in there.
        hand_index: usize,
        /// The tiles drawn from the bag to refill the hand, in draw order.
        drawn: DrawnTiles,
    },
    /// `player`, at `step` of their turn, passed it
    /// ([`Game::skip_turn`](crate::Game::skip_turn), also called by
    /// [`Game::skip_stuck_players`](crate::Game::skip_stuck_players)).
    Skipped { player: Color, step: TurnStep },
    /// `color` left the game ([`Game::retire_color`](crate::Game::retire_color)).
    /// Their hand is gone, so this cannot be undone.
    Retired { color: Color },
}
