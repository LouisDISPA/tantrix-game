use std::{
    iter::repeat,
    ops::{Add, Neg, Sub},
};

use enum_iterator::Sequence;
use enum_map::Enum;

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Position {
    x: i8,
    y: i8,
}

impl Add<Position> for Position {
    type Output = Self;

    fn add(mut self, rhs: Position) -> Self::Output {
        self.x += rhs.x;
        self.y += rhs.y;
        self
    }
}

impl Neg for Position {
    type Output = Self;

    fn neg(mut self) -> Self::Output {
        self.x = -self.x;
        self.y = -self.y;
        self
    }
}

impl Sub<Position> for Position {
    type Output = Self;

    fn sub(mut self, rhs: Position) -> Self::Output {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self
    }
}

impl Position {
    pub fn around(self) -> impl Iterator<Item = (Position, Direction)> {
        repeat(self)
            .zip(enum_iterator::all::<Direction>())
            .map(|(pos, dir)| (pos.neighbor(dir), dir))
    }

    pub fn neighbor(self, dir: Direction) -> Self {
        self + dir.delta()
    }
}

#[derive(Enum, Sequence, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum Rotation {
    None = 0,
    R60,
    R120,
    R180,
    R240,
    R300,
}

impl Rotation {
    const COUNT: i8 = enum_iterator::cardinality::<Self>() as i8;
}

#[derive(Enum, Sequence, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum Direction {
    Left = 0,
    TopLeft,
    TopRight,
    Right,
    BottomRight,
    BottomLeft,
}

impl From<i8> for Direction {
    fn from(index: i8) -> Self {
        match index.rem_euclid(Self::COUNT) {
            0 => Direction::Left,
            1 => Direction::TopLeft,
            2 => Direction::TopRight,
            3 => Direction::Right,
            4 => Direction::BottomRight,
            5 => Direction::BottomLeft,
            _ => unreachable!(),
        }
    }
}

impl From<Direction> for i8 {
    fn from(dir: Direction) -> Self {
        dir as i8
    }
}

impl Direction {
    const COUNT: i8 = enum_iterator::cardinality::<Self>() as i8;

    fn delta(self) -> Position {
        match self {
            Direction::Left => Position { x: -1, y: 0 },
            Direction::TopLeft => Position { x: 0, y: 1 },
            Direction::TopRight => Position { x: 1, y: 1 },
            Direction::Right => Position { x: 1, y: 0 },
            Direction::BottomRight => Position { x: 0, y: -1 },
            Direction::BottomLeft => Position { x: -1, y: -1 },
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn oposite(self) -> Self {
        (self as i8 + Self::COUNT / 2).into()
    }
}

impl From<i8> for Rotation {
    fn from(index: i8) -> Self {
        match index.rem_euclid(Self::COUNT) {
            0 => Rotation::None,
            1 => Rotation::R60,
            2 => Rotation::R120,
            3 => Rotation::R180,
            4 => Rotation::R240,
            5 => Rotation::R300,
            _ => unreachable!(),
        }
    }
}

impl From<Rotation> for i8 {
    fn from(rot: Rotation) -> Self {
        rot as i8
    }
}

impl Add<Rotation> for Direction {
    type Output = Self;

    fn add(self, rhs: Rotation) -> Self::Output {
        let mut val: i8 = self.into();
        val += i8::from(rhs);
        val.into()
    }
}

impl Sub<Rotation> for Direction {
    type Output = Self;

    fn sub(self, rhs: Rotation) -> Self::Output {
        let mut val: i8 = self.into();
        val -= i8::from(rhs);
        val.into()
    }
}

impl Position {
    pub const CENTER: Position = Position { x: 0, y: 0 };

    pub fn x(self) -> i8 {
        self.x
    }

    pub fn y(self) -> i8 {
        self.y
    }
}
