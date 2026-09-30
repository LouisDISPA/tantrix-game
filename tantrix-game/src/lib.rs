//! Tantrix rules: tiles, the board, legal moves, scoring and turns. No UI,
//! no I/O; the app, the TUI, the AIs and the room servers build on [`Game`].
//!
//! | Module | Holds |
//! |---|---|
//! | `tiles` | the 56 [`Tile`]s and their [`Color`]s |
//! | `coordinate_system` | [`Position`], [`Direction`], [`Rotation`] |
//! | `moves` | [`Placement`] (a player's input), [`Move`], [`PlayedMove`], [`TileMoveInfo`] |
//! | `board` | [`Board`] storage, lookups, contour, neighbor counts |
//! | `rules` | legal moves: golden rule, forced spaces, restriction rules |
//! | `scoring` | longest lines and loops: [`PlayerScore`], [`ScoringPath`] |
//! | `bag`, `hand` | [`Bag`], [`Hand`] |
//! | `turn` | [`TurnTracker`], [`TurnStep`] |
//! | `player` | [`Player`], [`TantrixAi`] |
//! | `game` | [`Game`], [`GameData`]: the single source of truth for a game |
//! | `error` | [`MoveError`], [`SetupError`] |

mod bag;
mod board;
mod coordinate_system;
mod error;
mod game;
mod hand;
mod moves;
mod player;
mod rules;
mod scoring;
mod tiles;
mod turn;

pub use bag::Bag;
pub use board::Board;
pub use coordinate_system::*;
pub use enum_map::EnumMap;
pub use error::{MoveError, SetupError};
pub use game::{is_game_over, Game, GameData};
pub use hand::{Hand, TILES_IN_HAND};
pub use moves::{Move, Placement, PlayedMove, TileMoveInfo};
pub use player::{Player, TantrixAi};
pub use scoring::{PlayerScore, ScoringPath};
pub use tiles::*;
pub use turn::{TurnStep, TurnTracker};
