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
            Self::Online(room) => room.is_connected() && room.my_color() == Some(color),
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
            Self::Online(room) => room.is_connected() && room.my_color().is_some(),
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
    match_id: ReadSignal<u32>,
    sender: Signal<Option<CommandSender>>,
    my_color: ReadSignal<Option<Color>>,
    names: ReadSignal<EnumMap<Color, Option<String>>>,
    connected: ReadSignal<bool>,
}

impl RoomLink {
    pub fn new(
        sender: Signal<Option<CommandSender>>,
        my_color: ReadSignal<Option<Color>>,
        names: ReadSignal<EnumMap<Color, Option<String>>>,
        connected: ReadSignal<bool>,
        match_id: ReadSignal<u32>,
    ) -> Self {
        Self {
            sender,
            match_id,
            my_color,
            names,
            connected,
        }
    }

    pub fn is_connected(self) -> bool {
        (self.connected)()
    }

    fn my_color(self) -> Option<Color> {
        (self.my_color)()
    }

    /// Ask the room to play `placement` for `player`, if that is this
    /// device's seat (the click handlers already check; this is the
    /// belt-and-braces one). The room's next snapshot shows the result.
    pub fn play(self, player: Color, placement: Placement) {
        if self.my_color() == Some(player) {
            self.send(SessionCommand::PlayMove {
                match_id: (self.match_id)(),
                placement,
            });
        }
    }

    /// Ask the room to end the game for everyone.
    pub fn end_game(self) {
        self.send(SessionCommand::EndGame {
            match_id: (self.match_id)(),
        });
    }

    fn send(self, command: SessionCommand) {
        if self.is_connected() && self.my_color().is_some() {
            if let Some(sender) = self.sender.read().as_ref() {
                sender.send(command);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tantrix_game::{Color, EnumMap, Placement, Position, Rotation, Tile};
    use tantrix_net::SessionCommand;

    use super::GameDriver;
    use crate::state::test_games::seeded;
    use crate::util::test_page::GamePage;

    const COLORS: [Color; 4] = [Color::Red, Color::Blue, Color::Yellow, Color::Green];

    fn names() -> EnumMap<Color, Option<String>> {
        EnumMap::from_fn(|color| (color == Color::Blue).then(|| "Bob".to_string()))
    }

    fn placement() -> Placement {
        Placement {
            tile: Tile::BYYBRR,
            position: Position::CENTER,
            rotation: Rotation::None,
        }
    }

    #[test]
    fn a_local_game_plays_every_seat_here() {
        let page = GamePage::local(seeded(3));
        let driver = page.ctx().driver;
        page.with(|_| {
            assert!(driver.runs_turns());
            assert!(COLORS.iter().all(|&color| driver.controls(color)));
            assert!(driver.can_end_game());
            assert_eq!(driver.my_color(), None);
            assert_eq!(driver.player_name(Color::Red), None);
        });
    }

    #[test]
    fn an_online_seat_plays_only_itself() {
        let (page, _commands) = GamePage::online(seeded(3), Some(Color::Blue), names());
        let driver = page.ctx().driver;
        page.with(|_| {
            // The room plays the turns: this device only sends its own.
            assert!(!driver.runs_turns());
            for color in COLORS {
                assert_eq!(driver.controls(color), color == Color::Blue, "{color}");
            }
            assert!(driver.can_end_game());
            assert_eq!(driver.my_color(), Some(Color::Blue));
            assert_eq!(driver.player_name(Color::Blue).as_deref(), Some("Bob"));
            assert_eq!(driver.player_name(Color::Red), None);
        });
    }

    #[test]
    fn a_watcher_plays_nothing_and_cannot_end_the_game() {
        let (page, _commands) = GamePage::online(seeded(3), None, names());
        let driver = page.ctx().driver;
        page.with(|_| {
            assert!(COLORS.iter().all(|&color| !driver.controls(color)));
            assert!(!driver.can_end_game());
            assert_eq!(driver.my_color(), None);
            // The seats' names show all the same.
            assert_eq!(driver.player_name(Color::Blue).as_deref(), Some("Bob"));
        });
    }

    #[test]
    fn the_room_hears_only_this_seats_moves() {
        let (page, mut commands) = GamePage::online(seeded(3), Some(Color::Blue), names());
        let GameDriver::Online(room) = page.ctx().driver else {
            panic!("an online page has an online driver");
        };
        page.with(|_| {
            room.play(Color::Red, placement());
            room.play(Color::Blue, placement());
            room.end_game();
        });
        let sent: Vec<_> = std::iter::from_fn(|| commands.try_recv().ok()).collect();
        assert!(
            matches!(
                sent.as_slice(),
                [SessionCommand::PlayMove { placement: p, .. }, SessionCommand::EndGame { match_id: 0 }] if *p == placement()
            ),
            "{sent:?}"
        );
    }
}
