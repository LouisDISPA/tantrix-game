use enum_iterator::IntoEnumIterator;
use enum_map::Enum;
use variant_count::VariantCount;

use crate::Direction;

#[derive(Enum, IntoEnumIterator, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Red,
    Bleu,
    Yellow,
    Green,
}

#[derive(VariantCount, IntoEnumIterator, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    RYYBRB,
    BYYBRR,
    YRRBBY,
    BYRBRY,
    YRBYBR,
    RBBYRY,
    YBBRYR,
    RBYRYB,
    BYYRBR,
    YRRBYB,
    BRRYBY,
    YRRYBB,
    RYYBBR,
}

impl Tile {
    pub fn into_colors_array(self) -> &'static [Color; 6] {
        use Color::{Bleu, Red, Yellow};

        match &self {
            Tile::RYYBRB => &[Red, Yellow, Yellow, Bleu, Red, Bleu],
            Tile::BYYBRR => &[Bleu, Yellow, Yellow, Bleu, Red, Red],
            Tile::YRRBBY => &[Yellow, Red, Red, Bleu, Bleu, Yellow],
            Tile::BYRBRY => &[Bleu, Yellow, Red, Bleu, Red, Yellow],
            Tile::YRBYBR => &[Yellow, Red, Bleu, Yellow, Bleu, Red],
            Tile::RBBYRY => &[Red, Bleu, Bleu, Yellow, Red, Yellow],
            Tile::YBBRYR => &[Yellow, Bleu, Bleu, Red, Yellow, Red],
            Tile::RBYRYB => &[Red, Bleu, Yellow, Red, Yellow, Bleu],
            Tile::BYYRBR => &[Bleu, Yellow, Yellow, Red, Bleu, Red],
            Tile::YRRBYB => &[Yellow, Red, Red, Bleu, Yellow, Bleu],
            Tile::BRRYBY => &[Bleu, Red, Red, Yellow, Bleu, Yellow],
            Tile::YRRYBB => &[Yellow, Red, Red, Yellow, Bleu, Bleu],
            Tile::RYYBBR => &[Red, Yellow, Yellow, Bleu, Bleu, Red],
        }
    }

    pub fn color(self, dir: Direction) -> Color {
        let array = self.into_colors_array();
        array[dir.index()]
    }
}
