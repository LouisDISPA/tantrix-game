//! Turn order: whose turn it is, and where they are within it.

use arrayvec::ArrayVec;
use enum_map::EnumMap;
use rand::prelude::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::tiles::NUMBER_OF_COLOR;
use crate::{Color, Player, SetupError};

/// Where the current player is within their turn: fill forced spaces, make
/// one free move, then fill forced spaces again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TurnStep {
    /// Forced spaces come first, then the free move.
    #[default]
    BeforeFreeMove,
    /// The free move is played; only forced spaces can still be filled.
    AfterFreeMove,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnTracker {
    currently_playing: usize,
    color_cycle: ArrayVec<Color, NUMBER_OF_COLOR>,
    /// Games saved before turn steps existed resume at the turn's start.
    #[serde(default)]
    step: TurnStep,
}

impl TurnTracker {
    pub fn new(players: &EnumMap<Color, Player>) -> Result<Self, SetupError> {
        Self::new_with_rng(players, &mut rand::rng())
    }

    /// A turn order shuffled by `rng`; a seeded `rng` always gives the same order.
    pub fn new_with_rng<R: Rng + ?Sized>(
        players: &EnumMap<Color, Player>,
        rng: &mut R,
    ) -> Result<Self, SetupError> {
        let mut color_cycle: ArrayVec<Color, NUMBER_OF_COLOR> = players
            .iter()
            .filter(|(_, p)| *p != &Player::None)
            .map(|(c, _)| c)
            .collect();
        if color_cycle.is_empty() {
            return Err(SetupError::EmptyTurnCycle);
        }
        color_cycle.shuffle(rng);
        Ok(TurnTracker {
            currently_playing: 0,
            color_cycle,
            step: TurnStep::default(),
        })
    }

    pub fn current(&self) -> Color {
        self.color_cycle[self.currently_playing]
    }

    pub fn step(&self) -> TurnStep {
        self.step
    }

    pub fn next(&mut self) {
        self.currently_playing += 1;
        self.currently_playing %= self.color_cycle.len();
        self.step = TurnStep::default();
    }

    pub fn playing_colors(&self) -> &[Color] {
        &self.color_cycle
    }

    /// Build a TurnTracker pointing at `current`, at `step` of their turn,
    /// with the given ordered color cycle. Used by clients that receive an
    /// authoritative snapshot and need to reconstruct turn state without
    /// reshuffling.
    pub fn from_parts(
        current: Color,
        colors: impl IntoIterator<Item = Color>,
        step: TurnStep,
    ) -> Result<Self, SetupError> {
        let color_cycle: ArrayVec<Color, NUMBER_OF_COLOR> = colors.into_iter().collect();
        if color_cycle.is_empty() {
            return Err(SetupError::EmptyTurnCycle);
        }
        let Some(currently_playing) = color_cycle.iter().position(|&c| c == current) else {
            return Err(SetupError::CurrentPlayerNotInCycle);
        };
        Ok(TurnTracker {
            currently_playing,
            color_cycle,
            step,
        })
    }

    /// The current player just made the free move of their turn.
    pub(crate) fn free_move_played(&mut self) {
        self.step = TurnStep::AfterFreeMove;
    }

    /// Drop `color` from the cycle, keeping the turn with the same player,
    /// or passing it to the next one (at the start of their turn) if it was
    /// `color`'s.
    pub(crate) fn retire(&mut self, color: Color) -> Result<(), &'static str> {
        let Some(idx) = self.color_cycle.iter().position(|&c| c == color) else {
            return Err("color is not in the turn cycle");
        };
        if self.color_cycle.len() <= 1 {
            return Err("cannot retire the last remaining player");
        }
        self.color_cycle.remove(idx);
        if self.currently_playing > idx {
            self.currently_playing -= 1;
        } else {
            if self.currently_playing == idx {
                self.step = TurnStep::default();
            }
            if self.currently_playing >= self.color_cycle.len() {
                self.currently_playing = 0;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Color, TurnStep, TurnTracker};

    #[test]
    fn saved_turn_without_step_resumes_at_turn_start() {
        let turn: TurnTracker =
            serde_json::from_str(r#"{"currently_playing":1,"color_cycle":["Red","Blue"]}"#)
                .unwrap();
        assert_eq!(turn.current(), Color::Blue);
        assert_eq!(turn.step(), TurnStep::BeforeFreeMove);
    }
}
