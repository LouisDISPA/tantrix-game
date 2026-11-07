//! A whole game: board, bag, hands, players and turn order. [`Game`] is the
//! single source of truth for a game in progress: every change goes through
//! it, and it keeps the current player's legal moves up to date. It records
//! every change as a [`TurnEvent`], so it can undo them, and a game dealt
//! from a seed can be replayed from its seed and events.
//!
//! | Module | Holds |
//! |---|---|
//! | `setup` | dealing, saving and restoring, mirroring a server snapshot |
//! | `queries` | reads: the table, legal moves, status, scores, winners |
//! | `turns` | changes: moves, skipped turns, AI turns, retirements |
//! | `history` | undo and replay of the recorded events |
//! | `ai` | what an AI is shown, and games it plays ahead |

mod ai;
mod history;
#[cfg(test)]
mod model_tests;
mod queries;
mod setup;
#[cfg(test)]
mod test_helpers;
mod turns;

use std::sync::OnceLock;

use enum_map::EnumMap;
use serde::{Deserialize, Serialize};

use crate::{Bag, Board, Color, Hand, Move, Player, TurnEvent, TurnTracker};

/// One game: board, bag, hands, players, turn order and history. Its
/// methods play by the rules and record every change.
#[derive(Debug, Clone)]
pub struct Game {
    board: Board,
    bag: Bag,
    players: EnumMap<Color, Player>,
    turn_tracker: TurnTracker,
    hands: EnumMap<Color, Hand>,
    /// The current player's legal moves at their step of the turn. Every
    /// change clears them, and the first read after it works them out once,
    /// so a game rebuilt only to be read or saved never computes them.
    legal_moves: OnceLock<Vec<Move>>,
    /// The seed the game was dealt from, when it is known.
    seed: Option<u64>,
    /// Every change since the deal, in order.
    history: Vec<TurnEvent>,
}

/// Complete server-side game state, including the hidden draw order (the
/// bag, and the seed that shuffled it). Never send this over the player
/// protocol; `tantrix_proto::GameSnapshot` omits the bag, seed and history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameData {
    board: Board,
    bag: Bag,
    turn_tracker: TurnTracker,
    hands: EnumMap<Color, Hand>,
    /// Games saved before seeds were kept have none.
    #[serde(default)]
    seed: Option<u64>,
    /// Games saved before history was kept resume with an empty one.
    #[serde(default)]
    history: Vec<TurnEvent>,
}

/// Whether a game goes on ([`Game::status`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameStatus {
    /// Someone can move: the current player, or a player after them once
    /// [`Game::skip_stuck_players`] passes the turn on.
    Playing,
    /// The bag and every hand are empty.
    Finished,
    /// Tiles are left, but none of them fits anywhere: nobody can move.
    Stalled,
}

impl Game {
    /// Forget the legal moves after a change: the next read works them out
    /// for the new state.
    fn clear_legal_moves(&mut self) {
        self.legal_moves = OnceLock::new();
    }
}
