//! Hex-tile components sharing a common SVG rendering primitive.
//!
//! * [`HexSvg`] — pure SVG helper that renders the tile polygon + color curves
//!   + optional scoring overlay. Reused by all tile-shaped components.
//! * [`BoardTile`] — absolutely-positioned tile on the board (placed or preview).
//! * [`EmptyHex`] — absolutely-positioned empty hex slot (click target).
//! * [`HandTile`] — interactive tile in a player's hand.

pub(crate) mod board_tile;
pub(crate) mod empty_hex;
pub(crate) mod hand_tile;
pub(crate) mod hex_svg;

pub(crate) use board_tile::{BoardTile, BoardTileVariant};
pub(crate) use empty_hex::EmptyHex;
pub(crate) use hand_tile::HandTile;
pub(crate) use hex_svg::HexSvg;
