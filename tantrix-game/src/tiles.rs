use std::fmt;

use enum_iterator::Sequence;
use enum_map::Enum;
use serde::{Deserialize, Serialize};

use crate::Direction;

#[derive(Enum, Sequence, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Color {
    Red,
    Blue,
    Yellow,
    Green,
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Sequence, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tile {
    // RYB tiles (1-14)
    RYYBRB, // 1: YYBRBR
    BYYBRR, // 2: YYBRRB
    YRRBBY, // 3: RRBBYY
    BYRBRY, // 4: YRBRYB
    RYYRBB, // 5: YYRBBR
    YRBYBR, // 6: RBYBRY
    RBBYRY, // 7: BBYRYR
    YBBRYR, // 8: BBRYRY
    RYBRBY, // 9: YBRBYR
    BYYRBR, // 10: YYRBRB
    YRRBYB, // 11: RRBYBY
    BRRYBY, // 12: RRYBYB
    YBBYRR, // 13: BBYRRY
    RYYBBR, // 14: YYBBRR
    // RYG tiles (15-23)
    RGGRYY, // 15: GGRYYR
    YGGYRR, // 16: GGYRRY
    RYYGRG, // 17: YYGRGR
    GYYRGR, // 18: YYRGRG
    YRRGYG, // 19: RRGYGY
    GRRYGY, // 20: RRYGYG
    YGGRRY, // 21: GGRRYY
    GYYGRR, // 22: YYGRRG
    GGYYRR, // 23: GYYRRG
    // RBG tiles (24-30)
    BGGBRR, // 24: GGBRRB
    BGGRRB, // 25: GGRRBB
    GBBGRR, // 26: BBGRRG
    GRRBGB, // 27: RRBGBG
    RGGBBR, // 28: GGBBRR
    BRRGBG, // 29: RRGBGB
    RGGRBB, // 30: GGRBBR
    // RYG mixed tiles (31-35)
    YGGRYR, // 31: GGRYRY
    GRYGYR, // 32: RYGYRG
    RGGYRY, // 33: GGYRYR
    RGYRYG, // 34: GYRYGR
    YGRYRG, // 35: GRYRGY
    // RBG mixed tiles (36-42)
    BGGRBR, // 36: GGRBRB
    RBBGRG, // 37: BBGRGR
    GBBRGR, // 38: BBRGRG
    GRBGBR, // 39: RBGBRG
    BRGBGR, // 40: RGBGRB
    RGGBRB, // 41: GGBRBR
    RBGRGB, // 42: BGRGBR
    // BYG tiles (43-56)
    YGGBBY, // 43: GGBBYY
    YBGYGB, // 44: BGYGBY
    BGGYYB, // 45: GGYYBB
    YGGBYB, // 46: GGBYBY
    YGGYBB, // 47: GGYBBY
    BGGBYY, // 48: GGBYYB
    GYYGBB, // 49: YYGBBG
    BYGBGY, // 50: YGBGYB
    GBYGYB, // 51: BYGYBG
    BGGYBY, // 52: GGYBYB
    BYYGBG, // 53: YYGBGB
    GYYBGB, // 54: YYBGBG
    YBBGYG, // 55: BBGYGY
    GBBYGY, // 56: BBYGYG
}

impl Tile {
    pub fn colors(self) -> &'static [Color; 6] {
        use Color::{Blue as B, Green as G, Red as R, Yellow as Y};

        match &self {
            // RYB tiles (1-14)
            Tile::RYYBRB => &[R, Y, Y, B, R, B],
            Tile::BYYBRR => &[B, Y, Y, B, R, R],
            Tile::YRRBBY => &[Y, R, R, B, B, Y],
            Tile::BYRBRY => &[B, Y, R, B, R, Y],
            Tile::RYYRBB => &[R, Y, Y, R, B, B],
            Tile::YRBYBR => &[Y, R, B, Y, B, R],
            Tile::RBBYRY => &[R, B, B, Y, R, Y],
            Tile::YBBRYR => &[Y, B, B, R, Y, R],
            Tile::RYBRBY => &[R, Y, B, R, B, Y],
            Tile::BYYRBR => &[B, Y, Y, R, B, R],
            Tile::YRRBYB => &[Y, R, R, B, Y, B],
            Tile::BRRYBY => &[B, R, R, Y, B, Y],
            Tile::YBBYRR => &[Y, B, B, Y, R, R],
            Tile::RYYBBR => &[R, Y, Y, B, B, R],
            // RYG tiles (15-23)
            Tile::RGGRYY => &[R, G, G, R, Y, Y],
            Tile::YGGYRR => &[Y, G, G, Y, R, R],
            Tile::RYYGRG => &[R, Y, Y, G, R, G],
            Tile::GYYRGR => &[G, Y, Y, R, G, R],
            Tile::YRRGYG => &[Y, R, R, G, Y, G],
            Tile::GRRYGY => &[G, R, R, Y, G, Y],
            Tile::YGGRRY => &[Y, G, G, R, R, Y],
            Tile::GYYGRR => &[G, Y, Y, G, R, R],
            Tile::GGYYRR => &[G, G, Y, Y, R, R],
            // RBG tiles (24-30)
            Tile::BGGBRR => &[B, G, G, B, R, R],
            Tile::BGGRRB => &[B, G, G, R, R, B],
            Tile::GBBGRR => &[G, B, B, G, R, R],
            Tile::GRRBGB => &[G, R, R, B, G, B],
            Tile::RGGBBR => &[R, G, G, B, B, R],
            Tile::BRRGBG => &[B, R, R, G, B, G],
            Tile::RGGRBB => &[R, G, G, R, B, B],
            // RYG mixed tiles (31-35)
            Tile::YGGRYR => &[Y, G, G, R, Y, R],
            Tile::GRYGYR => &[G, R, Y, G, Y, R],
            Tile::RGGYRY => &[R, G, G, Y, R, Y],
            Tile::RGYRYG => &[R, G, Y, R, Y, G],
            Tile::YGRYRG => &[Y, G, R, Y, R, G],
            // RBG mixed tiles (36-42)
            Tile::BGGRBR => &[B, G, G, R, B, R],
            Tile::RBBGRG => &[R, B, B, G, R, G],
            Tile::GBBRGR => &[G, B, B, R, G, R],
            Tile::GRBGBR => &[G, R, B, G, B, R],
            Tile::BRGBGR => &[B, R, G, B, G, R],
            Tile::RGGBRB => &[R, G, G, B, R, B],
            Tile::RBGRGB => &[R, B, G, R, G, B],
            // BYG tiles (43-56)
            Tile::YGGBBY => &[Y, G, G, B, B, Y],
            Tile::YBGYGB => &[Y, B, G, Y, G, B],
            Tile::BGGYYB => &[B, G, G, Y, Y, B],
            Tile::YGGBYB => &[Y, G, G, B, Y, B],
            Tile::YGGYBB => &[Y, G, G, Y, B, B],
            Tile::BGGBYY => &[B, G, G, B, Y, Y],
            Tile::GYYGBB => &[G, Y, Y, G, B, B],
            Tile::BYGBGY => &[B, Y, G, B, G, Y],
            Tile::GBYGYB => &[G, B, Y, G, Y, B],
            Tile::BGGYBY => &[B, G, G, Y, B, Y],
            Tile::BYYGBG => &[B, Y, Y, G, B, G],
            Tile::GYYBGB => &[G, Y, Y, B, G, B],
            Tile::YBBGYG => &[Y, B, B, G, Y, G],
            Tile::GBBYGY => &[G, B, B, Y, G, Y],
        }
    }

    pub fn color(self, dir: Direction) -> Color {
        let array = self.colors();
        array[dir.index()]
    }
}
