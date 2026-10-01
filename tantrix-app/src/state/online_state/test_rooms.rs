//! Test-only rooms: lobbies and game snapshots seating the given players.
//! Each player's peer id is their nickname.

use tantrix_game::{Board, Color, EnumMap, TurnStep};
use tantrix_proto::{GameSnapshot, LobbyState, Participant, ParticipantRole, PeerId, Seat};

/// A seated player: their color, nickname and whether they are connected.
pub type Sitter = (Color, &'static str, bool);

pub fn seat(name: &str) -> Seat {
    Seat {
        peer_id: PeerId::new(name),
        nickname: name.to_string(),
    }
}

fn seats(seated: &[Sitter]) -> EnumMap<Color, Option<Seat>> {
    EnumMap::from_fn(|color| {
        let (_, name, _) = seated.iter().find(|(c, _, _)| *c == color)?;
        Some(seat(name))
    })
}

/// The roster: `seated` (connected or not), then `unseated` (connected).
fn participants(seated: &[Sitter], unseated: &[&str]) -> Vec<Participant> {
    let seated = seated.iter().map(|&(color, name, connected)| Participant {
        peer_id: PeerId::new(name),
        nickname: name.to_string(),
        role: ParticipantRole::Seated(color),
        seated_color: Some(color),
        connected,
    });
    let unseated = unseated.iter().map(|&name| Participant {
        peer_id: PeerId::new(name),
        nickname: name.to_string(),
        role: ParticipantRole::Unseated,
        seated_color: None,
        connected: true,
    });
    seated.chain(unseated).collect()
}

/// A lobby with `seated` at their colors and `unseated` waiting.
pub fn lobby(seated: &[Sitter], unseated: &[&str]) -> LobbyState {
    LobbyState {
        seats: seats(seated),
        unseated: unseated.iter().map(|name| seat(name)).collect(),
        participants: participants(seated, unseated),
    }
}

/// A game just started by `seated` (in that turn order), with `watching`
/// in the room too.
pub fn snapshot(seated: &[Sitter], watching: &[&str]) -> GameSnapshot {
    GameSnapshot {
        board: Board::default(),
        hands: EnumMap::default(),
        bag_count: 44,
        current_player: seated[0].0,
        turn_step: TurnStep::BeforeFreeMove,
        playing_colors: seated.iter().map(|&(color, _, _)| color).collect(),
        seats: seats(seated),
        participants: participants(seated, watching),
    }
}
