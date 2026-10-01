//! Who plays the game on screen, so the same game screens serve a local game
//! and an online room.
//!
//! * [`GameDriver::Local`] — this device's [`tantrix_game::Game`] is the game:
//!   moves are played on it straight away, stuck players are skipped and AI
//!   seats are played here (see [`super::actions`]).
//! * [`GameDriver::Online`] — the room's game is authoritative: this device
//!   sends its seat's intents and mirrors the snapshots the room sends back.
//!
//! Components ask the driver (through `GameCtx`) what this device may do, and
//! the turn actions dispatch on it, so no component knows about the session.

use dioxus::prelude::*;
use tantrix_game::{Color, EnumMap, Placement};
use tantrix_net::SessionCommand;

use super::online_state::CommandSender;

#[derive(Clone, Copy)]
pub enum GameDriver {
    /// Every seat is played on this device: humans by clicks, AI seats by the
    /// AI loop.
    Local,
    /// A server-authoritative room.
    Online(RoomLink),
}

impl GameDriver {
    /// Whether this device runs the turn flow: skips stuck players and plays
    /// the AI seats. Online the room does, and this device only mirrors it.
    pub fn runs_turns(self) -> bool {
        matches!(self, Self::Local)
    }

    /// Whether a human on this device may play `color`'s tiles on its turn:
    /// every seat locally (AI seats wait for the AI loop), only its own seat
    /// online, and none while watching a room.
    pub fn controls(self, color: Color) -> bool {
        match self {
            Self::Local => true,
            Self::Online(room) => room.my_color() == Some(color),
        }
    }

    /// The seat this device plays in an online room. `None` locally, where
    /// every seat is played here, and while watching a room.
    pub fn my_color(self) -> Option<Color> {
        match self {
            Self::Local => None,
            Self::Online(room) => room.my_color(),
        }
    }

    /// Whether this device may end the game early: always locally, and online
    /// only from a seat (the room then ends it for everyone).
    pub fn can_end_game(self) -> bool {
        match self {
            Self::Local => true,
            Self::Online(room) => room.my_color().is_some(),
        }
    }

    /// The nickname of whoever sits at `color` in an online room.
    pub fn player_name(self, color: Color) -> Option<String> {
        match self {
            Self::Local => None,
            Self::Online(room) => room.names.read()[color].clone(),
        }
    }
}

/// This device's side of an online room: the session's command channel, and
/// the seats as the room's latest snapshot shows them.
#[derive(Clone, Copy)]
pub struct RoomLink {
    sender: Signal<Option<CommandSender>>,
    my_color: Memo<Option<Color>>,
    names: Memo<EnumMap<Color, Option<String>>>,
}

impl RoomLink {
    pub fn new(
        sender: Signal<Option<CommandSender>>,
        my_color: Memo<Option<Color>>,
        names: Memo<EnumMap<Color, Option<String>>>,
    ) -> Self {
        Self {
            sender,
            my_color,
            names,
        }
    }

    fn my_color(self) -> Option<Color> {
        (self.my_color)()
    }

    /// Ask the room to play `placement` for `player`, if that is this
    /// device's seat (the click handlers already check; this is the
    /// belt-and-braces one). The room's next snapshot shows the result.
    pub fn play(self, player: Color, placement: Placement) {
        if self.my_color() == Some(player) {
            self.send(SessionCommand::PlayMove(placement));
        }
    }

    /// Ask the room to end the game for everyone.
    pub fn end_game(self) {
        self.send(SessionCommand::EndGame);
    }

    fn send(self, command: SessionCommand) {
        if let Some(sender) = self.sender.read().as_ref() {
            sender.send(command);
        }
    }
}
