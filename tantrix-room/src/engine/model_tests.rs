//! Model-based tests: random sequences of socket events run through a fake
//! server the way both servers run them. Sockets open, say `Hello` (or a
//! name the room refuses), send intents (in turn or not, for this match or
//! another), malformed, oversized and binary frames, and close; the server
//! fails to reach some, fails a save, restarts or wakes from hibernation;
//! the clock moves for the rate limit. The test keeps its own model of the
//! sockets: which are open, which were welcomed, how many frames each may
//! still send. After every event:
//!
//! - storage holds exactly the room the engine committed;
//! - the engine's sockets are the model's, joined once welcomed;
//! - each member has at most one joined socket, members without a seat
//!   have one, and seats are held by members only;
//! - a room keeps at most [`MAX_CONNECTIONS`] sockets;
//! - no message carries a session token, a seed or a tile of the bag;
//! - every room saved resumes after a restart (`RoomEngine::new(saved, [])`
//!   then `on_load`) as a consistent room.
//!
//! Events are abstract (the n-th open socket, the current player's n-th
//! legal move), so any sequence applies to any room, and proptest shrinks a
//! failure to a short one. Cases come from a fixed seed.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use proptest::prelude::*;
use proptest::test_runner::{Config, FileFailurePersistence, RngSeed, TestRunner};
use serde_json::Value;
use tantrix_game::{Color, Direction, Game, GameStatus, Placement, Position, Rotation, Tile};
use tantrix_proto::{HostMessage, PeerMessage};

use super::*;
use crate::room_logic::{clean_nickname, DEAL_SEED, MAX_NICKNAME_CHARS};

/// People with a session cookie of their own; floods add more.
const PEOPLE: usize = 4;
const COLORS: [Color; 4] = [Color::Red, Color::Blue, Color::Yellow, Color::Green];
/// What every session token starts with, to find one in a message.
const TOKEN_PREFIX: &str = "session-token-";

fn token(owner: &str) -> SessionToken {
    SessionToken::new(format!("{TOKEN_PREFIX}{owner}-0123456789"))
}

#[derive(Debug, Clone, Copy)]
enum Name {
    Valid(usize),
    Blank,
    TooLong,
}

const NAMES: [&str; 7] = ["Ann", "Bo", "Cyd", "Dee", "Émile", "李", "🦊 Fox"];

impl Name {
    fn text(self) -> String {
        match self {
            Self::Valid(i) => NAMES[i % NAMES.len()].into(),
            Self::Blank => " \u{200B}\u{202E} ".into(),
            Self::TooLong => "n".repeat(MAX_NICKNAME_CHARS + 1),
        }
    }
}

/// Text frames that are not a message.
const JUNK: [&str; 8] = [
    "",
    "{",
    "null",
    "\"RequestSnapshot\" trailing",
    "{\"Hello\":{}}",
    "{\"PickColor\":{\"color\":\"Purple\"}}",
    "{\"PlayMove\":{\"match_id\":0}}",
    "{\"Shout\":{\"text\":\"hi\"}}",
];

#[derive(Debug, Clone)]
enum Frame {
    Hello(Name),
    PickColor(Option<Color>),
    ReplaceSeat(Color),
    /// `match_id` is the room's plus this.
    StartGame(i32),
    /// The current player's legal move at this index (wrapped), for the
    /// room's match plus the offset, sent by whoever.
    PlayMove(usize, i32),
    /// A placement no game allows: far from every tile.
    IllegalMove,
    EndGame(i32),
    Rematch(i32),
    RequestSnapshot,
    UpdateNickname(Name),
    Junk(usize),
    /// Longer than [`MAX_MESSAGE_BYTES`].
    Oversized,
}

#[derive(Debug, Clone)]
enum Event {
    /// A new socket for person `who`.
    Open {
        who: usize,
    },
    /// The `sock`-th open socket (wrapped) sends `frame`.
    Send {
        sock: usize,
        frame: Frame,
    },
    /// The `nth` joined socket of a seated player (wrapped) sends `frame`.
    Seated {
        nth: usize,
        frame: Frame,
    },
    /// The current player's socket, if one is joined, plays their legal
    /// move at this index (wrapped).
    Play(usize),
    /// The `sock`-th open socket sends `frames` frames at once.
    Burst {
        sock: usize,
        frames: u64,
    },
    Binary {
        sock: usize,
    },
    /// The `sock`-th open socket closes.
    Close {
        sock: usize,
    },
    /// A frame, a close and a failed delivery for a socket the engine
    /// doesn't have: the `nth` closed one, or one never opened.
    Ghost {
        nth: usize,
    },
    /// The server can't hand the `sock`-th open socket anything any more.
    Slow {
        sock: usize,
    },
    /// The clock moves, back too.
    Tick(i64),
    /// The next save fails.
    FailSave,
    /// The room is loaded again: after a restart, without its sockets, or
    /// after the Durable Object hibernated, with them.
    Reload {
        keep_sockets: bool,
    },
    /// `count` new people open a socket, and say `Hello` if `hello`.
    Flood {
        count: usize,
        hello: bool,
    },
}

fn name() -> impl Strategy<Value = Name> {
    prop_oneof![
        6 => any::<usize>().prop_map(Name::Valid),
        1 => Just(Name::Blank),
        1 => Just(Name::TooLong),
    ]
}

fn color() -> impl Strategy<Value = Color> {
    prop::sample::select(&COLORS[..])
}

/// Mostly this match, sometimes the one before or after.
fn match_offset() -> impl Strategy<Value = i32> {
    prop_oneof![6 => Just(0), 1 => Just(-1), 1 => Just(1)]
}

fn frame() -> impl Strategy<Value = Frame> {
    prop_oneof![
        5 => name().prop_map(Frame::Hello),
        5 => prop::option::weighted(0.8, color()).prop_map(Frame::PickColor),
        1 => color().prop_map(Frame::ReplaceSeat),
        3 => match_offset().prop_map(Frame::StartGame),
        1 => (any::<usize>(), match_offset()).prop_map(|(i, o)| Frame::PlayMove(i, o)),
        1 => Just(Frame::IllegalMove),
        2 => match_offset().prop_map(Frame::EndGame),
        3 => match_offset().prop_map(Frame::Rematch),
        1 => Just(Frame::RequestSnapshot),
        1 => name().prop_map(Frame::UpdateNickname),
        1 => any::<usize>().prop_map(Frame::Junk),
        1 => Just(Frame::Oversized),
    ]
}

fn event() -> impl Strategy<Value = Event> {
    let sock = any::<usize>;
    prop_oneof![
        4 => (0..PEOPLE).prop_map(|who| Event::Open { who }),
        14 => (sock(), frame()).prop_map(|(sock, frame)| Event::Send { sock, frame }),
        8 => (any::<usize>(), frame()).prop_map(|(nth, frame)| Event::Seated { nth, frame }),
        12 => any::<usize>().prop_map(Event::Play),
        1 => (sock(), 1..=2 * MESSAGE_BURST).prop_map(|(sock, frames)| Event::Burst { sock, frames }),
        1 => sock().prop_map(|sock| Event::Binary { sock }),
        2 => sock().prop_map(|sock| Event::Close { sock }),
        1 => any::<usize>().prop_map(|nth| Event::Ghost { nth }),
        1 => sock().prop_map(|sock| Event::Slow { sock }),
        2 => (-3_000..20_000i64).prop_map(Event::Tick),
        // Each closes most sockets: rarer, so that games get going.
        1 => prop_oneof![
            Just(Event::FailSave),
            any::<bool>().prop_map(|keep_sockets| Event::Reload { keep_sockets }),
            (1..=MAX_CONNECTIONS + 8, any::<bool>())
                .prop_map(|(count, hello)| Event::Flood { count, hello }),
        ],
    ]
}

/// The events that seat the first `players` people and start a game.
fn game_start(players: usize) -> Vec<Event> {
    let mut events = Vec::new();
    for (who, &color) in COLORS.iter().enumerate().take(players) {
        events.push(Event::Open { who });
        let send = |frame| Event::Send { sock: who, frame };
        events.push(send(Frame::Hello(Name::Valid(who))));
        events.push(send(Frame::PickColor(Some(color))));
    }
    events.push(Event::Send {
        sock: 0,
        frame: Frame::StartGame(0),
    });
    events
}

/// The server: storage, and sockets it may fail to reach.
#[derive(Default)]
struct Server {
    /// What storage holds.
    storage: RoomData,
    /// The rooms saved since the last check.
    saves: Vec<RoomData>,
    fail_next_save: bool,
    /// The effects delivered since the last check, in order.
    delivered: Vec<Effect>,
    /// Sockets whose queue is full: anything sent to them comes back.
    slow: BTreeSet<ConnId>,
}

impl RoomHost for Server {
    type Error = ();

    fn save(&mut self, room: &RoomData) -> impl Future<Output = Result<(), ()>> {
        let saved = if std::mem::take(&mut self.fail_next_save) {
            Err(())
        } else {
            self.storage = room.clone();
            self.saves.push(room.clone());
            Ok(())
        };
        std::future::ready(saved)
    }

    fn deliver(&mut self, effects: Vec<Effect>) -> Vec<ConnId> {
        let mut unreachable = Vec::new();
        for effect in &effects {
            for conn in recipients(effect) {
                if self.slow.contains(&conn) && !unreachable.contains(&conn) {
                    unreachable.push(conn);
                }
            }
        }
        self.delivered.extend(effects);
        unreachable
    }
}

fn recipients(effect: &Effect) -> Vec<ConnId> {
    match effect {
        Effect::Send { to, .. } => vec![*to],
        Effect::Broadcast { to, .. } => to.clone(),
        Effect::Close { .. } => Vec::new(),
    }
}

/// The fake server never waits, so one poll finishes a run.
fn now<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("the fake server never waits"),
    }
}

/// A socket as the model sees it.
struct Socket {
    token: SessionToken,
    /// The engine has it.
    open: bool,
    /// Its client closed it.
    gone: bool,
    /// It was sent a `Welcome` that let it in.
    welcomed: bool,
    /// What is left of its rate limit, once it sent a frame:
    /// `(thousandths of a frame, when)`.
    allowance: Option<(u64, u64)>,
}

/// How much the cases reached, summed over them.
#[derive(Debug, Default)]
struct Coverage {
    welcomed: usize,
    refused: usize,
    games_started: usize,
    moves: usize,
    games_ended: usize,
    rematches: usize,
    /// By close reason.
    closes: BTreeMap<&'static str, usize>,
    failed_saves: usize,
    resumed: usize,
}

struct Harness<'a> {
    engine: RoomEngine,
    server: Server,
    /// By connection number.
    sockets: Vec<Socket>,
    clock: u64,
    visitors: usize,
    /// The room's match when last checked.
    match_id: u32,
    coverage: &'a mut Coverage,
}

/// Fixed seed, few cases: the same few seconds every run.
fn config(cases: u32) -> Config {
    Config {
        cases,
        rng_seed: RngSeed::Fixed(0x2004_5eed),
        // Proptest's default stops at four tries per case.
        max_shrink_iters: 4096,
        source_file: Some(file!()),
        failure_persistence: Some(Box::new(FileFailurePersistence::SourceParallel(
            "proptest-regressions",
        ))),
        ..Config::default()
    }
}

#[test]
fn random_socket_events_keep_the_room_consistent() {
    let coverage = RefCell::new(Coverage::default());
    // Half the cases start in a game between two to four people.
    let cases = (
        any::<u64>(),
        prop::option::of(2..=4usize),
        prop::collection::vec(event(), 0..150),
    );
    TestRunner::new(config(64))
        .run(&cases, |(deal_seed, players, events)| {
            DEAL_SEED.set(deal_seed);
            let mut coverage = coverage.borrow_mut();
            let mut harness = Harness::new(&mut coverage);
            harness.check()?;
            let start = players.map(game_start).unwrap_or_default();
            for event in start.iter().chain(&events) {
                harness.apply(event)?;
                harness.check()?;
            }
            Ok(())
        })
        .unwrap();

    let c = coverage.into_inner();
    println!("{c:?}");
    for reason in [
        OPENED_ELSEWHERE,
        STORAGE_UNAVAILABLE,
        TOO_SLOW,
        TOO_MANY_MESSAGES,
        TOO_MANY_CONNECTIONS,
        "",
    ] {
        assert!(c.closes.contains_key(reason), "no {reason:?} close: {c:?}");
    }
    for reached in [
        c.welcomed,
        c.refused,
        c.games_started,
        c.moves,
        c.games_ended,
        c.rematches,
        c.failed_saves,
        c.resumed,
    ] {
        assert!(reached > 0, "{c:?}");
    }
}

impl<'a> Harness<'a> {
    fn new(coverage: &'a mut Coverage) -> Self {
        Self {
            engine: RoomEngine::default(),
            server: Server::default(),
            sockets: Vec::new(),
            clock: 0,
            visitors: 0,
            match_id: 0,
            coverage,
        }
    }

    fn apply(&mut self, event: &Event) -> Result<(), TestCaseError> {
        match *event {
            Event::Open { who } => {
                self.open(token(&format!("person-{who}")))?;
            }
            Event::Send { sock, ref frame } => {
                if let Some(conn) = self.nth_open(sock) {
                    self.send(conn, frame)?;
                }
            }
            Event::Seated { nth, ref frame } => {
                if let Some(conn) = self.nth_seated(nth) {
                    self.send(conn, frame)?;
                }
            }
            Event::Play(index) => {
                if let Some(conn) = self.current_player_socket() {
                    self.send(conn, &Frame::PlayMove(index, 0))?;
                }
            }
            Event::Burst { sock, frames } => {
                if let Some(conn) = self.nth_open(sock) {
                    for _ in 0..frames {
                        if self.sockets[conn.0 as usize].open {
                            self.send(conn, &Frame::RequestSnapshot)?;
                        }
                    }
                }
            }
            Event::Binary { sock } => {
                if let Some(conn) = self.nth_open(sock) {
                    self.on_frame(conn, true, |engine, now| engine.on_binary(conn, now))?;
                }
            }
            Event::Close { sock } => {
                if let Some(conn) = self.nth_open(sock) {
                    // Nothing may be sent to it any more. The engine keeps
                    // it until the step that drops it commits, so a failed
                    // save closes it with the others.
                    self.sockets[conn.0 as usize].gone = true;
                    let step = self.engine.on_close(conn);
                    self.run(step)?;
                    self.sockets[conn.0 as usize].open = false;
                }
            }
            Event::Ghost { nth } => {
                let closed = self.conns(false);
                let conn = if closed.is_empty() {
                    ConnId(self.sockets.len() as u64 + 1000)
                } else {
                    closed[nth % closed.len()]
                };
                let hello = self.frame_text(&Frame::Hello(Name::Valid(nth)));
                for step in [
                    self.engine.on_text(conn, &hello, self.clock),
                    self.engine.on_binary(conn, self.clock),
                    self.engine.on_close(conn),
                    self.engine.on_unreachable(&[conn]),
                ] {
                    prop_assert!(step.persist().is_none(), "{:?} changed the room", conn);
                    prop_assert!(step.effects().is_empty(), "{:?} had effects", conn);
                    self.run(step)?;
                }
            }
            Event::Slow { sock } => {
                if let Some(conn) = self.nth_open(sock) {
                    self.server.slow.insert(conn);
                }
            }
            Event::Tick(ms) => self.clock = self.clock.saturating_add_signed(ms),
            Event::FailSave => self.server.fail_next_save = true,
            Event::Reload { keep_sockets } => self.reload(keep_sockets)?,
            Event::Flood { count, hello } => {
                for _ in 0..count {
                    self.visitors += 1;
                    let conn = self.open(token(&format!("visitor-{}", self.visitors)))?;
                    if hello && self.sockets[conn.0 as usize].open {
                        self.send(conn, &Frame::Hello(Name::Valid(self.visitors)))?;
                    }
                }
            }
        }
        Ok(())
    }

    /// The sockets the model has open, or those it has closed.
    fn conns(&self, open: bool) -> Vec<ConnId> {
        let numbered = self.sockets.iter().zip(0..).map(|(s, n)| (s, ConnId(n)));
        numbered
            .filter(|(socket, _)| socket.open == open)
            .map(|(_, conn)| conn)
            .collect()
    }

    /// The `n`-th open socket, wrapped.
    fn nth_open(&self, n: usize) -> Option<ConnId> {
        let open = self.conns(true);
        (!open.is_empty()).then(|| open[n % open.len()])
    }

    /// The `n`-th joined socket of a seated player, wrapped.
    fn nth_seated(&self, n: usize) -> Option<ConnId> {
        let room = self.engine.room();
        let seated: Vec<ConnId> = self
            .engine
            .connections()
            .filter(|(_, c)| c.joined && room.color_of(&c.token).is_some())
            .map(|(conn, _)| conn)
            .collect();
        (!seated.is_empty()).then(|| seated[n % seated.len()])
    }

    /// The joined socket of the player whose turn it is, if any.
    fn current_player_socket(&self) -> Option<ConnId> {
        let room = self.engine.room();
        let game = Game::from_data(room.game.clone()?);
        let seat = room.seats[game.current_player()].as_ref()?;
        self.engine
            .connections()
            .find(|(_, c)| c.joined && &c.token == seat)
            .map(|(conn, _)| conn)
    }

    fn frame_text(&self, frame: &Frame) -> String {
        let room = self.engine.room();
        let match_id = |offset: i32| room.match_id.wrapping_add_signed(offset);
        let message = match *frame {
            Frame::Hello(name) => PeerMessage::Hello {
                nickname: name.text(),
            },
            Frame::PickColor(color) => PeerMessage::PickColor { color },
            Frame::ReplaceSeat(color) => PeerMessage::ReplaceSeat { color },
            Frame::StartGame(offset) => PeerMessage::StartGame {
                match_id: match_id(offset),
            },
            Frame::PlayMove(index, offset) => {
                let legal = room
                    .game
                    .clone()
                    .map(|data| Game::from_data(data).legal_moves().to_vec())
                    .unwrap_or_default();
                let Some(m) = legal.get(index % legal.len().max(1)) else {
                    return self.frame_text(&Frame::IllegalMove);
                };
                PeerMessage::PlayMove {
                    match_id: match_id(offset),
                    placement: m.placement(),
                }
            }
            Frame::IllegalMove => PeerMessage::PlayMove {
                match_id: room.match_id,
                placement: Placement {
                    tile: Tile::BYYBRR,
                    position: (0..40).fold(Position::CENTER, |p, _| p.neighbor(Direction::Right)),
                    rotation: Rotation::None,
                },
            },
            Frame::EndGame(offset) => PeerMessage::EndGame {
                match_id: match_id(offset),
            },
            Frame::Rematch(offset) => PeerMessage::Rematch {
                match_id: match_id(offset),
            },
            Frame::RequestSnapshot => PeerMessage::RequestSnapshot,
            Frame::UpdateNickname(name) => PeerMessage::UpdateNickname {
                nickname: name.text(),
            },
            Frame::Junk(i) => return JUNK[i % JUNK.len()].into(),
            Frame::Oversized => PeerMessage::Hello {
                nickname: "n".repeat(MAX_MESSAGE_BYTES),
            },
        };
        wire(&message)
    }

    /// Open a socket for `token`; it may be closed at once if the room
    /// has no room for it.
    /// The room is full of sockets: the oldest that never said `Hello` is
    /// closed for the new one, or the new one if there is none.
    fn open(&mut self, token: SessionToken) -> Result<ConnId, TestCaseError> {
        let conn = ConnId(self.sockets.len() as u64);
        let open = self.conns(true);
        let make_room = (open.len() >= MAX_CONNECTIONS).then(|| {
            let waiting = open
                .into_iter()
                .find(|c| !self.sockets[c.0 as usize].welcomed);
            waiting.unwrap_or(conn)
        });
        self.sockets.push(Socket {
            token: token.clone(),
            open: true,
            gone: false,
            welcomed: false,
            allowance: None,
        });
        let step = self.engine.on_open(conn, token);
        let closed = step.effects().iter().find_map(|effect| match effect {
            Effect::Close { conn, reason } if *reason == TOO_MANY_CONNECTIONS => Some(*conn),
            _ => None,
        });
        prop_assert_eq!(closed, make_room, "the socket cap");
        self.run(step)?;
        Ok(conn)
    }

    /// `frame` from `conn`, as text. Frames that are not a message, moves
    /// no game allows and a nickname the sender has already change nothing.
    fn send(&mut self, conn: ConnId, frame: &Frame) -> Result<(), TestCaseError> {
        let quiet = match frame {
            Frame::Junk(_) | Frame::Oversized | Frame::IllegalMove => true,
            Frame::UpdateNickname(name) => {
                let socket = &self.sockets[conn.0 as usize];
                let member = self.engine.room().player(&socket.token);
                socket.welcomed
                    && member.map(|m| &m.nickname) == clean_nickname(&name.text()).as_ref()
            }
            _ => false,
        };
        let text = self.frame_text(frame);
        self.on_frame(conn, quiet, |engine, now| engine.on_text(conn, &text, now))
    }

    /// A frame from `conn` at the current time: the model's rate limit says
    /// whether the engine lets it through or closes the socket. One let
    /// through that is `quiet` saves nothing and is answered to its sender
    /// alone.
    fn on_frame(
        &mut self,
        conn: ConnId,
        quiet: bool,
        receive: impl FnOnce(&RoomEngine, u64) -> Step,
    ) -> Result<(), TestCaseError> {
        let socket = &mut self.sockets[conn.0 as usize];
        let allowed = spend(socket.allowance, self.clock);
        if let Some(left) = allowed {
            socket.allowance = Some(left);
        }
        let step = receive(&self.engine, self.clock);
        let too_fast = step.effects().iter().any(|effect| {
            matches!(effect, Effect::Close { conn: c, reason } if *c == conn && *reason == TOO_MANY_MESSAGES)
        });
        prop_assert_eq!(too_fast, allowed.is_none(), "rate limit of {:?}", conn);
        if quiet && allowed.is_some() {
            prop_assert!(step.persist().is_none(), "a quiet frame saved the room");
            let to_sender =
                |effect: &Effect| matches!(effect, Effect::Send { to, .. } if *to == conn);
            prop_assert!(step.effects().iter().all(to_sender), "{:?}", step.effects());
        }
        self.run(step)
    }

    /// Run `step` as a server does, and follow its effects in the model.
    /// A failed save closes every socket, and the server loads the room
    /// again.
    fn run(&mut self, step: Step) -> Result<(), TestCaseError> {
        let ran = now(self.engine.run(&mut self.server, step));
        let delivered = std::mem::take(&mut self.server.delivered);
        self.follow(&delivered)?;
        if ran.is_err() {
            self.coverage.failed_saves += 1;
            prop_assert!(
                self.sockets.iter().all(|s| !s.open),
                "a socket outlived a failed save"
            );
            prop_assert_eq!(self.engine.connections().count(), 0);
            prop_assert_eq!(
                json(self.engine.room()),
                json(&self.server.storage),
                "a failed save committed the room"
            );
            self.reload(false)?;
        }
        Ok(())
    }

    /// The room is loaded from storage, with the sockets still open if
    /// `keep_sockets` (their rate limits start again), or after a restart
    /// that closed them all.
    fn reload(&mut self, keep_sockets: bool) -> Result<(), TestCaseError> {
        let connections: Vec<(ConnId, Connection)> = if keep_sockets {
            self.engine
                .connections()
                .map(|(conn, c)| (conn, c.clone()))
                .collect()
        } else {
            Vec::new()
        };
        for socket in &mut self.sockets {
            socket.allowance = None;
            socket.open &= keep_sockets;
        }
        if !keep_sockets {
            self.server.slow.clear();
        }
        self.engine = RoomEngine::new(self.server.storage.clone(), connections);
        let step = self.engine.on_load();
        self.run(step)
    }

    /// Follow `effects` in the model: what each socket was sent, and which
    /// closed. Each message is checked for secrets.
    fn follow(&mut self, effects: &[Effect]) -> Result<(), TestCaseError> {
        let messages = effects.iter().any(|e| !matches!(e, Effect::Close { .. }));
        let bag = if messages {
            bag_tiles(self.engine.room())
        } else {
            HashSet::new()
        };
        for effect in effects {
            match effect {
                Effect::Send { to, message } => {
                    check_secrets(message, &bag)?;
                    self.receive(*to, message, false)?;
                }
                Effect::Broadcast { to, message } => {
                    check_secrets(message, &bag)?;
                    for &conn in to {
                        self.receive(conn, message, true)?;
                    }
                }
                Effect::Close { conn, reason } => {
                    let socket = &mut self.sockets[conn.0 as usize];
                    prop_assert!(socket.open, "{:?} closed twice", conn);
                    socket.open = false;
                    *self.coverage.closes.entry(reason).or_default() += 1;
                }
            }
        }
        Ok(())
    }

    fn receive(
        &mut self,
        conn: ConnId,
        message: &HostMessage,
        broadcast: bool,
    ) -> Result<(), TestCaseError> {
        let socket = &mut self.sockets[conn.0 as usize];
        prop_assert!(
            socket.open && !socket.gone,
            "{:?} was sent {:?} once closed",
            conn,
            message
        );
        // A socket hears nothing but errors and its welcome until it is in.
        let welcome = matches!(message, HostMessage::Welcome { .. });
        prop_assert!(
            socket.welcomed
                || welcome
                || (!broadcast && matches!(message, HostMessage::Error { .. })),
            "{:?} was sent {:?} before its welcome",
            conn,
            message
        );
        if let HostMessage::Welcome { rejected, .. } = message {
            prop_assert!(!socket.welcomed, "{:?} welcomed twice", conn);
            match rejected {
                None => {
                    socket.welcomed = true;
                    self.coverage.welcomed += 1;
                }
                Some(_) => self.coverage.refused += 1,
            }
        }
        let reached = match message {
            HostMessage::GameStarted { .. } if broadcast => &mut self.coverage.games_started,
            HostMessage::StateUpdate {
                last_move: Some(_), ..
            } => &mut self.coverage.moves,
            HostMessage::GameEnded { .. } if broadcast => &mut self.coverage.games_ended,
            _ => return Ok(()),
        };
        *reached += 1;
        Ok(())
    }

    /// What must hold between events.
    fn check(&mut self) -> Result<(), TestCaseError> {
        let room = self.engine.room();
        prop_assert_eq!(
            json(room),
            json(&self.server.storage),
            "storage holds another room"
        );
        check_room(room)?;

        // The engine's sockets are the model's.
        let engine: BTreeMap<ConnId, (&SessionToken, bool)> = self
            .engine
            .connections()
            .map(|(conn, c)| (conn, (&c.token, c.joined)))
            .collect();
        let model: BTreeMap<ConnId, (&SessionToken, bool)> = self
            .conns(true)
            .into_iter()
            .map(|conn| {
                let socket = &self.sockets[conn.0 as usize];
                (conn, (&socket.token, socket.welcomed))
            })
            .collect();
        prop_assert_eq!(&engine, &model);
        prop_assert!(engine.len() <= MAX_CONNECTIONS, "{} sockets", engine.len());

        // One joined socket per member at most, and only members join.
        let mut joined: HashMap<&SessionToken, usize> = HashMap::new();
        for (token, _) in engine.values().filter(|(_, joined)| *joined) {
            *joined.entry(token).or_default() += 1;
        }
        for (token, count) in &joined {
            prop_assert_eq!(*count, 1, "{} joined sockets", count);
            prop_assert!(room.player(token).is_some(), "a joined socket of no member");
        }
        // A member without a seat leaves with their last socket.
        for player in &room.players {
            if room.color_of(&player.token).is_none() {
                prop_assert!(
                    joined.contains_key(&player.token),
                    "{} stayed without a seat or a socket",
                    player.nickname
                );
            }
        }

        // Every room saved resumes after a restart.
        for saved in std::mem::take(&mut self.server.saves) {
            check_resume(&saved)?;
            self.coverage.resumed += 1;
        }
        if room.match_id > self.match_id {
            self.coverage.rematches += 1;
            self.match_id = room.match_id;
        }
        Ok(())
    }
}

/// `allowance` after one more frame at `now`, or `None` when the socket has
/// sent too many: [`MESSAGE_BURST`] at once, then [`MESSAGES_PER_SECOND`].
/// A clock that goes back refills nothing.
fn spend(allowance: Option<(u64, u64)>, now: u64) -> Option<(u64, u64)> {
    let full = MESSAGE_BURST * 1000;
    let (left, at) = allowance.unwrap_or((full, now));
    let refilled = (left + now.saturating_sub(at) * MESSAGES_PER_SECOND).min(full);
    Some((refilled.checked_sub(1000)?, now.max(at)))
}

/// Things a room keeps true however it got there.
fn check_room(room: &RoomData) -> Result<(), TestCaseError> {
    prop_assert!(room.players.len() <= MAX_MEMBERS);
    let tokens: HashSet<&SessionToken> = room.players.iter().map(|p| &p.token).collect();
    let ids: HashSet<&str> = room.players.iter().map(|p| p.id.as_str()).collect();
    prop_assert_eq!(tokens.len(), room.players.len(), "a member twice");
    prop_assert_eq!(ids.len(), room.players.len(), "a peer id twice");
    for player in &room.players {
        let cleaned = clean_nickname(&player.nickname);
        prop_assert_eq!(cleaned.as_ref(), Some(&player.nickname));
    }

    // Seats are held by members, one each.
    let seated: Vec<(Color, &SessionToken)> = room
        .seats
        .iter()
        .filter_map(|(color, seat)| seat.as_ref().map(|token| (color, token)))
        .collect();
    for (color, token) in &seated {
        prop_assert!(tokens.contains(token), "{:?} is held by no member", color);
    }
    let holders: HashSet<_> = seated.iter().map(|(_, token)| token).collect();
    prop_assert_eq!(holders.len(), seated.len(), "a member with two seats");

    match &room.game {
        Some(data) => {
            let game = Game::from_data(data.clone());
            prop_assert_eq!(game.seed(), None, "an online deal kept a seed");
            let mut playing = game.playing_colors().to_vec();
            playing.sort_by_key(|&c| c as u8);
            let colors: Vec<Color> = seated.iter().map(|&(color, _)| color).collect();
            prop_assert_eq!(colors, playing, "seats and players differ");
            if !room.ended {
                // Stuck players are skipped as soon as a turn ends.
                prop_assert_eq!(game.status(), GameStatus::Playing);
                prop_assert!(!game.legal_moves().is_empty(), "a stuck player's turn");
            }
        }
        None => prop_assert!(!room.ended, "a game ended that never started"),
    }
    prop_assert!(room.end_reason.is_none() || room.ended);

    // The room can say what it holds.
    let nobody = HashSet::new();
    let _ = room.current_message(&nobody);
    let _ = room.roster_message(&nobody);
    let _ = room.completed_match();
    Ok(())
}

/// `saved` loaded after a restart, with no socket left: only seated
/// members stay, all of them, loading it again changes nothing, and each
/// of them can come back.
fn check_resume(saved: &RoomData) -> Result<(), TestCaseError> {
    let mut engine = RoomEngine::new(saved.clone(), []);
    let step = engine.on_load();
    prop_assert!(step.effects().is_empty(), "no socket to tell");
    let _ = engine.commit(step);
    let room = engine.room();
    check_room(room)?;
    let seated: Vec<&str> = saved
        .players
        .iter()
        .filter(|p| saved.color_of(&p.token).is_some())
        .map(|p| p.id.as_str())
        .collect();
    let stayed: Vec<&str> = room.players.iter().map(|p| p.id.as_str()).collect();
    prop_assert_eq!(stayed, seated);
    prop_assert_eq!(json(&room.seats), json(&saved.seats));
    prop_assert_eq!(json(&room.game), json(&saved.game));
    let again = engine.on_load();
    prop_assert!(again.persist().is_none() && again.effects().is_empty());

    // Each of them comes back to their seat, without a save.
    let hello = wire(&PeerMessage::Hello {
        nickname: "Back".into(),
    });
    for (n, player) in saved.players.iter().enumerate() {
        if saved.color_of(&player.token).is_none() {
            continue;
        }
        let conn = ConnId(n as u64);
        let opened = engine.on_open(conn, player.token.clone());
        let _ = engine.commit(opened);
        let back = engine.on_text(conn, &hello, 0);
        prop_assert!(
            back.persist().is_none(),
            "{} came back with a save",
            player.nickname
        );
        let welcomed = back.effects().iter().any(|effect| {
            matches!(effect, Effect::Send { to, message: HostMessage::Welcome { your_peer_id, rejected: None, .. } }
                if *to == conn && *your_peer_id == player.id)
        });
        prop_assert!(welcomed, "{} was not welcomed back", player.nickname);
        let _ = engine.commit(back);
    }
    Ok(())
}

/// No message names a session token, holds a seed, a bag or a history, or
/// names a tile still in the bag.
fn check_secrets(message: &HostMessage, bag: &HashSet<String>) -> Result<(), TestCaseError> {
    let text = wire(message);
    prop_assert!(!text.contains(TOKEN_PREFIX), "a session token in {}", text);
    let mut values = vec![serde_json::from_str::<Value>(&text).unwrap()];
    while let Some(value) = values.pop() {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    prop_assert!(
                        !["seed", "bag", "history"].contains(&key.as_str()),
                        "{} in {}",
                        key,
                        text
                    );
                    values.push(value);
                }
            }
            Value::Array(items) => values.extend(items),
            Value::String(s) => prop_assert!(!bag.contains(&s), "{} of the bag in {}", s, text),
            _ => {}
        }
    }
    Ok(())
}

/// The names of the tiles in the room's bag.
fn bag_tiles(room: &RoomData) -> HashSet<String> {
    let Some(data) = &room.game else {
        return HashSet::new();
    };
    let saved = serde_json::to_value(data).unwrap();
    saved["bag"]["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tile| tile.as_str().unwrap().to_string())
        .collect()
}

/// `message` as the servers send it.
fn wire(message: &impl Serialize) -> String {
    String::from_utf8(tantrix_proto::encode(message)).unwrap()
}

fn json(value: &impl Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}
