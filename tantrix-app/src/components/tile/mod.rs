//! Hex-tile components sharing a common SVG rendering primitive.
//!
//! * [`HexSvg`] — pure SVG helper that renders the tile polygon + color curves
//!   + optional scoring overlay. Reused by all tile-shaped components.
//! * [`BoardTile`] — absolutely-positioned tile on the board (placed or preview).
//! * [`EmptyHex`] — absolutely-positioned empty hex slot (click target).
//! * [`HandTile`] — interactive tile in a player's hand.
//! * [`FocusHex`] — the keyboard's focus ring round a tile or a spot.

pub(crate) mod board_tile;
pub(crate) mod empty_hex;
pub(crate) mod focus_hex;
pub(crate) mod hand_tile;
pub(crate) mod hex_svg;

pub(crate) use board_tile::{BoardTile, BoardTileVariant};
pub(crate) use empty_hex::{spot_id, EmptyHex};
pub(crate) use focus_hex::FocusHex;
pub(crate) use hand_tile::HandTile;
pub(crate) use hex_svg::HexSvg;
