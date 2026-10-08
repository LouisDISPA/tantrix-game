//! Tantrix rules: tiles, the board, legal moves, scoring and turns. No UI,
//! no I/O; the app, the TUI, the AIs and the room servers build on [`Game`].
//!
//! | Module | Holds |
//! |---|---|
//! | `tiles` | the 56 [`Tile`]s and their [`Color`]s |
//! | `edges` | [`Edges`]: a hex's edge colors, packed for fast fit checks |
//! | `coordinate_system` | [`Position`], [`Direction`], [`Rotation`] |
//! | `moves` | [`Placement`] (a player's input), [`Move`], [`PlayedMove`], [`TileMoveInfo`] |
//! | `board` | [`Board`] storage, lookups, contour, neighbor counts |
//! | `rules` | legal moves: golden rule, forced spaces, restriction rules |
//! | `scoring` | longest lines and loops: [`PlayerScore`], [`ScoringPath`], every [`Line`] |
//! | `bag`, `hand` | [`Bag`], [`Hand`] |
//! | `turn` | [`TurnTracker`], [`TurnStep`] |
//! | `player` | [`Player`], [`TantrixAi`], [`AiView`] |
//! | `game` | [`Game`], [`GameData`]: the single source of truth for a game |
//! | `history` | [`TurnEvent`]: the recorded moves and skips, for undo and replay |
//! | `error` | [`MoveError`], [`SetupError`], [`EventError`], [`ReplayError`] |

mod bag;
mod board;
mod coordinate_system;
mod edges;
mod error;
mod game;
mod grid;
mod hand;
mod history;
mod moves;
mod player;
mod rules;
mod scoring;
#[cfg(test)]
mod test_games;
mod tiles;
mod turn;

pub use bag::Bag;
pub use board::Board;
pub use coordinate_system::*;
pub use edges::Edges;
pub use enum_map::EnumMap;
pub use error::{EventError, MoveError, ReplayError, SetupError};
pub use game::{Game, GameData};
pub use hand::{DrawnTiles, Hand, TILES_IN_HAND};
pub use history::TurnEvent;
pub use moves::{Move, Placement, PlayedMove, TileMoveInfo};
pub use player::{AiView, Player, TantrixAi};
pub use scoring::{IndexedBoard, Line, LineEnd, PlayerScore, ScoringPath};
pub use tiles::*;
pub use turn::{TurnStep, TurnTracker};
