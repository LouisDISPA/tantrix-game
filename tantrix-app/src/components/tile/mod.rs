//! Hex-tile components sharing a common SVG rendering primitive.
//!
//! * [`HexSvg`] — pure SVG helper that renders the tile polygon + color curves
//!   + optional scoring overlay. Reused by all tile-shaped components.
//! * [`BoardTile`] — absolutely-positioned tile on the board (placed or preview).
//! * [`EmptyHex`] — absolutely-positioned empty hex slot (click target).
//! * [`HandTile`] — interactive tile in a player's hand.

pub mod board_tile;
pub mod empty_hex;
pub mod hand_tile;
pub mod hex_svg;

pub use board_tile::{BoardTile, BoardTileVariant};
pub use empty_hex::EmptyHex;
pub use hand_tile::{HandTile, HandTileAction};
pub use hex_svg::HexSvg;
