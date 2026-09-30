use std::collections::HashSet;

use rand::rngs::Xoshiro256PlusPlus;
use rand::SeedableRng;
use serde::Deserialize;
use tantrix_game::{Color, Direction, Game, Move, Player, Position, Rotation, Tile, TurnStep};
use tantrix_proto::{HostMessage, PeerId, PeerMessage, RejectCode};
use tantrix_room::room_logic::{RoomData, MAX_MEMBERS};
use tantrix_room::session_cookie::{self, SessionToken};

/// The seeded two-player game the browser test plays: a free move creates a
/// forced space its player can fill.
const FORCED_AFTER_FREE_MOVE: &str = include_str!("../../e2e/fixtures/forced-after-free-move.json");

#[derive(Deserialize)]
struct Fixture {
    seed: u64,
    moves: Vec<FixtureMove>,
}

#[derive(Deserialize)]
struct FixtureMove {
    player: Color,
    tile: Tile,
    position: Position,
    rotation: Rotation,
    forced: bool,
    next: Color,
}

impl FixtureMove {
    fn intent(&self) -> PeerMessage {
        PeerMessage::PlayMove {
            m: Move {
                tile: self.tile,
                position: self.position,
                rotation: self.rotation,
                forced: self.forced,
            },
        }
    }
}

fn setup() -> (RoomData, SessionToken, SessionToken, HashSet<SessionToken>) {
    let mut room = RoomData::default();
    let red = SessionToken::new("secret-red-123456789");
    let blue = SessionToken::new("secret-blue-12345678");
    room.hello(red.clone(), "Red".into(), PeerId::new("public-red"))
        .unwrap();
    room.hello(blue.clone(), "Blue".into(), PeerId::new("public-blue"))
        .unwrap();
    let connected = HashSet::from([red.clone(), blue.clone()]);
    room.apply(
        &red,
        PeerMessage::PickColor {
            color: Some(Color::Red),
        },
        &connected,
    )
    .unwrap();
    room.apply(
        &blue,
        PeerMessage::PickColor {
            color: Some(Color::Blue),
        },
        &connected,
    )
    .unwrap();
    (room, red, blue, connected)
}

#[test]
fn server_validates_turn_and_move_then_persists_hidden_state() {
    let (mut room, red, blue, connected) = setup();
    let started = room
        .apply(&blue, PeerMessage::StartGame, &connected)
        .unwrap();
    assert!(matches!(started.message, HostMessage::GameStarted { .. }));
    let snapshot = room.snapshot(&connected).unwrap();
    let (turn, other) = if snapshot.current_player == Color::Red {
        (&red, &blue)
    } else {
        (&blue, &red)
    };
    let tile = snapshot.hands[snapshot.current_player][0];
    let invalid = Move {
        tile,
        position: Position::default()
            .neighbor(Direction::Left)
            .neighbor(Direction::Left),
        rotation: Rotation::None,
        forced: false,
    };
    assert_eq!(
        room.apply(other, PeerMessage::PlayMove { m: invalid }, &connected)
            .err()
            .unwrap()
            .0,
        RejectCode::NotYourTurn
    );
    assert_eq!(
        room.apply(turn, PeerMessage::PlayMove { m: invalid }, &connected)
            .err()
            .unwrap()
            .0,
        RejectCode::InvalidMove
    );
    assert!(room.snapshot(&connected).unwrap().board.is_empty());

    let saved = serde_json::to_string(&room).unwrap();
    let restored: RoomData = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored.snapshot(&connected), room.snapshot(&connected));
    let wire = serde_json::to_string(&room.snapshot(&connected).unwrap()).unwrap();
    assert!(!wire.contains(red.as_str()));
    assert!(!wire.contains(blue.as_str()));
}

#[test]
fn server_keeps_the_turn_to_fill_a_forced_space_after_the_free_move() {
    let (mut room, red, blue, connected) = setup();
    let fixture: Fixture = serde_json::from_str(FORCED_AFTER_FREE_MOVE).unwrap();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(fixture.seed);
    let game = Game::new_with_rng(
        Player::Someone,
        Player::Someone,
        Player::None,
        Player::None,
        &mut rng,
    )
    .unwrap();
    room.game = Some(game.to_data());
    let token = |color| if color == Color::Red { &red } else { &blue };

    let (free_move_turn, [fill]) = fixture.moves.split_at(fixture.moves.len() - 1) else {
        panic!("the fixture ends with the forced fill");
    };
    for step in free_move_turn {
        room.apply(token(step.player), step.intent(), &connected)
            .unwrap();
        assert_eq!(room.snapshot(&connected).unwrap().current_player, step.next);
    }

    // The free move is played but a forced space is left: the same player
    // keeps the turn, and clients learn they may only fill forced spaces.
    let snapshot = room.snapshot(&connected).unwrap();
    assert_eq!(snapshot.current_player, fill.player);
    assert_eq!(snapshot.turn_step, TurnStep::AfterFreeMove);
    let other = token(if fill.player == Color::Red {
        Color::Blue
    } else {
        Color::Red
    });
    assert_eq!(
        room.apply(other, fill.intent(), &connected)
            .err()
            .unwrap()
            .0,
        RejectCode::NotYourTurn
    );

    room.apply(token(fill.player), fill.intent(), &connected)
        .unwrap();
    let snapshot = room.snapshot(&connected).unwrap();
    assert_eq!(snapshot.current_player, fill.next);
    assert_eq!(snapshot.turn_step, TurnStep::BeforeFreeMove);
}

#[test]
fn started_game_reserves_disconnected_seat_for_token_reconnect() {
    let (mut room, red, blue, connected) = setup();
    room.apply(&red, PeerMessage::StartGame, &connected)
        .unwrap();
    assert!(!room.disconnect(&blue));
    let offline = HashSet::from([red.clone()]);
    let snapshot = room.snapshot(&offline).unwrap();
    let blue_participant = snapshot
        .participants
        .iter()
        .find(|p| p.peer_id.as_str() == "public-blue")
        .unwrap();
    assert!(!blue_participant.connected);
    assert_eq!(blue_participant.seated_color, Some(Color::Blue));

    let saved = serde_json::to_string(&room).unwrap();
    let mut restored: RoomData = serde_json::from_str(&saved).unwrap();
    let id = restored
        .hello(
            blue.clone(),
            "Ignored reload name".into(),
            PeerId::new("new-id"),
        )
        .unwrap();
    assert_eq!(id.as_str(), "public-blue");
    assert_eq!(restored.snapshot(&connected).unwrap().participants.len(), 2);
    // A newcomer mid-game joins without a seat and watches.
    let late = SessionToken::new("unknown-secret-123456");
    restored
        .hello(late.clone(), "Late".into(), PeerId::new("late-id"))
        .unwrap();
    assert_eq!(restored.color_of(&late), None);
    assert_eq!(restored.snapshot(&connected).unwrap().participants.len(), 3);
}

#[test]
fn a_watcher_can_replace_a_player_mid_game_and_inherit_the_turn() {
    let (mut room, red, blue, connected) = setup();
    room.apply(&red, PeerMessage::StartGame, &connected)
        .unwrap();
    let watcher = join(&mut room, "watcher");
    let online = HashSet::from([red.clone(), watcher.clone()]);
    let current = room.snapshot(&online).unwrap().current_player;
    let (current_holder, other_color) = if current == Color::Red {
        (red.clone(), Color::Blue)
    } else {
        (blue.clone(), Color::Red)
    };

    // A seated player can't grab a second seat; an unplayed color is refused.
    let reject = |room: &mut RoomData, who: &SessionToken, color| {
        room.apply(who, PeerMessage::ReplaceSeat { color }, &online)
            .err()
            .unwrap()
            .0
    };
    assert_eq!(
        reject(&mut room, &red, other_color),
        RejectCode::InvalidMove
    );
    assert_eq!(
        reject(&mut room, &watcher, Color::Green),
        RejectCode::InvalidMove
    );

    // The watcher takes the seat whose turn it is, and may now play it.
    let hand_before = room.snapshot(&online).unwrap().hands[current].clone();
    let change = room
        .apply(
            &watcher,
            PeerMessage::ReplaceSeat { color: current },
            &online,
        )
        .unwrap();
    assert!(matches!(change.message, HostMessage::StateUpdate { .. }));
    assert_eq!(room.color_of(&watcher), Some(current));
    assert_eq!(room.snapshot(&online).unwrap().hands[current], hand_before);
    // The replaced player can no longer act for that seat.
    assert!(room
        .apply(&current_holder, PeerMessage::SkipTurn, &online)
        .is_err());
    assert_ne!(
        room.apply(&watcher, PeerMessage::SkipTurn, &online)
            .err()
            .unwrap()
            .0,
        RejectCode::NotYourTurn
    );
    // Online replaced players stay to watch; offline ones leave.
    assert_eq!(
        room.player(&current_holder).is_some(),
        online.contains(&current_holder)
    );
}

#[test]
fn lobby_disconnect_keeps_seat_until_someone_else_takes_it() {
    let (mut room, red, blue, _) = setup();
    let watcher = join(&mut room, "watcher");
    let online = HashSet::from([red.clone(), watcher.clone()]);

    // Blue drops (e.g. reloads): the seat is kept, shown offline.
    assert!(!room.disconnect(&blue));
    let lobby = room.lobby(&online);
    assert!(lobby.seats[Color::Blue].is_some());
    assert!(
        !lobby
            .participants
            .iter()
            .find(|p| p.seated_color == Some(Color::Blue))
            .unwrap()
            .connected
    );
    assert!(room.apply(&red, PeerMessage::StartGame, &online).is_err());

    // A reconnect with the same cookie gets the same player and seat back.
    let id = room
        .hello(blue.clone(), "Blue".into(), PeerId::new("other"))
        .unwrap();
    assert_eq!(id.as_str(), "public-blue");
    assert_eq!(room.color_of(&blue), Some(Color::Blue));

    // Picking a held seat is refused, even an offline one, so two people
    // racing for a free seat can't replace each other by accident.
    assert_eq!(
        room.apply(
            &watcher,
            PeerMessage::PickColor {
                color: Some(Color::Blue)
            },
            &online
        )
        .err()
        .unwrap()
        .0,
        RejectCode::ColorTaken
    );

    // Replacing on purpose works: an offline Blue leaves the room...
    room.apply(
        &watcher,
        PeerMessage::ReplaceSeat { color: Color::Blue },
        &online,
    )
    .unwrap();
    assert_eq!(room.color_of(&watcher), Some(Color::Blue));
    assert!(room.player(&blue).is_none());

    // ...and an online Red is moved out of the seat but stays in the room.
    let newcomer = join(&mut room, "newcomer");
    let online = HashSet::from([red.clone(), watcher.clone(), newcomer.clone()]);
    room.apply(
        &newcomer,
        PeerMessage::ReplaceSeat { color: Color::Red },
        &online,
    )
    .unwrap();
    assert_eq!(room.color_of(&newcomer), Some(Color::Red));
    assert_eq!(room.color_of(&red), None);
    assert!(room.player(&red).is_some());

    // Moving to another held seat frees the one you had.
    room.apply(
        &newcomer,
        PeerMessage::ReplaceSeat { color: Color::Blue },
        &online,
    )
    .unwrap();
    assert_eq!(room.color_of(&newcomer), Some(Color::Blue));
    assert!(room.lobby(&online).seats[Color::Red].is_none());

    // Unseated members have nothing to keep and leave on disconnect.
    let passerby = join(&mut room, "passerby");
    assert!(room.disconnect(&passerby));
    assert!(room.player(&passerby).is_none());
}

fn join(room: &mut RoomData, name: &str) -> SessionToken {
    let token = SessionToken::new(format!("secret-{name}-0123456789"));
    room.hello(
        token.clone(),
        name.into(),
        PeerId::new(format!("public-{name}")),
    )
    .unwrap();
    token
}

fn pick(
    room: &mut RoomData,
    token: &SessionToken,
    color: Option<Color>,
    connected: &HashSet<SessionToken>,
) {
    room.apply(token, PeerMessage::PickColor { color }, connected)
        .unwrap();
}

#[test]
fn more_than_four_members_share_four_seats_by_releasing_them() {
    let mut room = RoomData::default();
    let tokens: Vec<_> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|name| join(&mut room, name))
        .collect();
    let connected: HashSet<_> = tokens.iter().cloned().collect();
    for (token, color) in tokens
        .iter()
        .zip([Color::Red, Color::Blue, Color::Yellow, Color::Green])
    {
        pick(&mut room, token, Some(color), &connected);
    }
    let (fifth, sixth) = (&tokens[4], &tokens[5]);
    assert_eq!(
        room.apply(
            fifth,
            PeerMessage::PickColor {
                color: Some(Color::Red)
            },
            &connected
        )
        .err()
        .unwrap()
        .0,
        RejectCode::ColorTaken
    );
    assert_eq!(room.lobby(&connected).unseated.len(), 2);

    // Red leaves its seat for the fifth member, who can then start.
    pick(&mut room, &tokens[0], None, &connected);
    pick(&mut room, fifth, Some(Color::Red), &connected);
    assert_eq!(room.color_of(fifth), Some(Color::Red));
    assert!(room
        .apply(sixth, PeerMessage::StartGame, &connected)
        .is_err());
    room.apply(fifth, PeerMessage::StartGame, &connected)
        .unwrap();

    // Unseated members stay in the room and watch, but can never move.
    let snapshot = room.snapshot(&connected).unwrap();
    assert_eq!(snapshot.participants.len(), 6);
    assert_eq!(snapshot.playing_colors.len(), 4);
    assert_eq!(
        room.apply(sixth, PeerMessage::SkipTurn, &connected)
            .err()
            .unwrap()
            .0,
        RejectCode::NotYourTurn
    );
}

#[test]
fn room_rejects_members_beyond_the_abuse_cap() {
    let mut room = RoomData::default();
    for i in 0..MAX_MEMBERS {
        join(&mut room, &format!("p{i}"));
    }
    assert_eq!(
        room.hello(
            SessionToken::new("one-too-many-0123456789"),
            "Late".into(),
            PeerId::new("late")
        ),
        Err(RejectCode::RoomFull)
    );
}

#[test]
fn session_cookie_is_parsed_strictly_and_set_http_only() {
    let token = "0f8fad5b-d9cb-469f-a165-70867728950e";
    let header = format!("theme=dark; tantrix_session={token}; other=1");
    assert_eq!(
        session_cookie::read(Some(&header)),
        Some(SessionToken::new(token))
    );
    assert_eq!(session_cookie::read(None), None);
    assert_eq!(session_cookie::read(Some("tantrix_session=short")), None);
    assert_eq!(
        session_cookie::read(Some("tantrix_session=bad;value-0123456789")),
        None
    );
    assert_eq!(
        session_cookie::read(Some("xtantrix_session=0f8fad5b-d9cb-469f")),
        None
    );

    let generated = SessionToken::generate();
    let set = session_cookie::set_cookie(&generated, true);
    assert!(set.starts_with(&format!("tantrix_session={}", generated.as_str())));
    assert!(set.contains("HttpOnly") && set.contains("SameSite=Lax") && set.ends_with("Secure"));
    assert!(!session_cookie::set_cookie(&generated, false).contains("Secure"));
    assert_eq!(
        session_cookie::read(Some(&format!("tantrix_session={}", generated.as_str()))),
        Some(generated)
    );
}
