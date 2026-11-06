//! Model-based tests: random sequences of actions on seeded games of two to
//! four players, some of them AIs. After every action the game must still
//! undo, replay and save exactly, report a status that its hands and bag
//! agree with, and offer only moves the rules allow, checked plainly from
//! the tiles around them.
//!
//! The actions are abstract (the n-th legal move, the n-th playing color),
//! so any sequence applies to any game, and proptest shrinks a failure to a
//! short one. Cases come from a fixed seed. A failing case is saved under
//! `tantrix-game/proptest-regressions/` and replayed first on every run:
//! check the file in with the fix.

use std::cell::Cell;
use std::collections::HashSet;

use enum_map::EnumMap;
use proptest::prelude::*;
use proptest::test_runner::{Config, FileFailurePersistence, RngSeed, TestRunner};
use serde_json::Value;

use crate::test_games::{broken_rule, tiles_of};
use crate::tiles::NUMBER_OF_TILE;
use crate::{
    AiView, Color, Game, GameStatus, Move, Player, RetireError, TantrixAi, TurnEvent, TurnStep,
    TILES_IN_HAND,
};

/// Who sits at a color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seat {
    Empty,
    Person,
    Ai,
}

#[derive(Debug, Clone)]
enum Action {
    /// Play the current player's legal move at this index (wrapped).
    Play(usize),
    /// [`Game::skip_stuck_players`].
    SkipStuck,
    /// [`Game::skip_turn`], whatever the current player could play.
    SkipTurn,
    /// [`Game::advance`]: the AIs play until a person's turn.
    Advance,
    /// Retire the playing color at this index (wrapped).
    Retire(usize),
    /// Undo this many events, or until nothing can be undone.
    Undo(usize),
}

fn table() -> impl Strategy<Value = [Seat; 4]> {
    let seat = prop_oneof![Just(Seat::Empty), Just(Seat::Person), Just(Seat::Ai)];
    prop::array::uniform4(seat).prop_filter("two players or more", |seats| {
        seats.iter().filter(|&&s| s != Seat::Empty).count() >= 2
    })
}

fn action() -> impl Strategy<Value = Action> {
    prop_oneof![
        14 => any::<usize>().prop_map(Action::Play),
        2 => Just(Action::SkipStuck),
        1 => Just(Action::SkipTurn),
        2 => Just(Action::Advance),
        1 => any::<usize>().prop_map(Action::Retire),
        2 => (1..=3usize).prop_map(Action::Undo),
    ]
}

/// Plays a legal move picked from what it sees: the same in every game.
#[derive(Clone)]
struct PickyAi;

impl TantrixAi for PickyAi {
    fn name(&self) -> &str {
        "picky"
    }

    fn clone_box(&self) -> Box<dyn TantrixAi> {
        Box::new(self.clone())
    }

    fn choose_move(&self, view: &AiView) -> Move {
        let pick = view.board.len() * 7 + view.bag_count;
        view.legal_moves[pick % view.legal_moves.len()]
    }
}

/// What the test remembers of a game, to check it against.
struct Model {
    seed: u64,
    players: EnumMap<Color, Player>,
    /// The saved game after each event, by history length; `None` between
    /// the events of one action, whose states are not seen.
    states: Vec<Option<Value>>,
    /// Tiles that left the game with a retired player's hand.
    discarded: usize,
}

/// How much of the game the cases reached, summed over them.
#[derive(Debug, Default)]
struct Coverage {
    free_moves: Cell<usize>,
    forced_before_free_move: Cell<usize>,
    forced_after_free_move: Cell<usize>,
    moves_from_empty_bag: Cell<usize>,
    ai_events: Cell<usize>,
    skips: Cell<usize>,
    undone: Cell<usize>,
    retired: Cell<usize>,
    finished: Cell<usize>,
}

fn bump(counter: &Cell<usize>, by: usize) {
    counter.set(counter.get() + by);
}

/// Everything the game saves: board, bag in draw order, hands, turn, seed
/// and history.
fn saved(game: &Game) -> Value {
    serde_json::to_value(game.to_data()).unwrap()
}

/// Fixed seed, few cases: the same few seconds every run.
fn config(cases: u32) -> Config {
    Config {
        cases,
        rng_seed: RngSeed::Fixed(0x7a27_41c5),
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
fn random_actions_keep_the_game_consistent() {
    let coverage = Coverage::default();
    let cases = (
        table(),
        any::<u64>(),
        prop::collection::vec(action(), 0..160),
    );
    TestRunner::new(config(16))
        .run(&cases, |(seats, seed, actions)| {
            play(seats, seed, &actions, &coverage)
        })
        .unwrap();

    println!("{coverage:?}");
    let reached = |counter: &Cell<usize>| counter.get() > 0;
    assert!(reached(&coverage.free_moves), "{coverage:?}");
    assert!(reached(&coverage.forced_before_free_move), "{coverage:?}");
    assert!(reached(&coverage.forced_after_free_move), "{coverage:?}");
    assert!(reached(&coverage.moves_from_empty_bag), "{coverage:?}");
    assert!(reached(&coverage.ai_events), "{coverage:?}");
    assert!(reached(&coverage.skips), "{coverage:?}");
    assert!(reached(&coverage.undone), "{coverage:?}");
    assert!(reached(&coverage.retired), "{coverage:?}");
    assert!(reached(&coverage.finished), "{coverage:?}");
}

fn play(
    seats: [Seat; 4],
    seed: u64,
    actions: &[Action],
    coverage: &Coverage,
) -> Result<(), TestCaseError> {
    let players = EnumMap::from_fn(|color: Color| match seats[color as usize] {
        Seat::Empty => Player::None,
        Seat::Person => Player::Someone,
        Seat::Ai => Player::Ai(Box::new(PickyAi)),
    });
    let mut game = Game::from_players_seeded(players.clone(), seed).unwrap();
    let mut model = Model {
        seed,
        players,
        states: vec![Some(saved(&game))],
        discarded: 0,
    };
    check(&game, &model)?;
    for action in actions {
        apply(&mut game, &mut model, action, coverage)?;
        check(&game, &model)?;
        if matches!(action, Action::Undo(_)) {
            check_replay(&game, &model)?;
        }
    }
    check_replay(&game, &model)?;
    bump(
        &coverage.finished,
        usize::from(game.status() == GameStatus::Finished),
    );
    Ok(())
}

fn apply(
    game: &mut Game,
    model: &mut Model,
    action: &Action,
    coverage: &Coverage,
) -> Result<(), TestCaseError> {
    let before = saved(game);
    let recorded = game.history().len();
    match *action {
        Action::Play(index) => {
            let legal = game.legal_moves();
            if legal.is_empty() {
                return Ok(());
            }
            let m = legal[index % legal.len()];
            let step = game.turn_step();
            let from_empty_bag = game.bag_is_empty();
            let played = game.play_move(m.placement());
            prop_assert_eq!(played.map(|p| p.m), Ok(m));
            let counter = match (m.forced, step) {
                (false, _) => &coverage.free_moves,
                (true, TurnStep::BeforeFreeMove) => &coverage.forced_before_free_move,
                (true, TurnStep::AfterFreeMove) => &coverage.forced_after_free_move,
            };
            bump(counter, 1);
            bump(&coverage.moves_from_empty_bag, usize::from(from_empty_bag));
        }
        Action::SkipStuck => {
            game.skip_stuck_players();
            bump(&coverage.skips, game.history().len() - recorded);
        }
        Action::SkipTurn => {
            game.skip_turn();
            bump(&coverage.skips, 1);
        }
        Action::Advance => {
            // The AIs only pick legal moves, which the game never refuses.
            prop_assert_eq!(game.advance(), Ok(()));
            bump(&coverage.ai_events, game.history().len() - recorded);
        }
        Action::Retire(index) => {
            let colors = game.playing_colors();
            let color = colors[index % colors.len()];
            let hand = game.hand(color).len();
            match game.retire_color(color) {
                Ok(()) => {
                    model.discarded += hand;
                    bump(&coverage.retired, 1);
                }
                Err(error) => {
                    prop_assert_eq!(error, RetireError::LastPlayer);
                    prop_assert_eq!(game.playing_colors(), &[color]);
                }
            }
        }
        Action::Undo(count) => {
            for _ in 0..count {
                let last = game.history().last().cloned();
                let unchanged = saved(game);
                let undone = game.undo();
                match last {
                    None | Some(TurnEvent::Retired { .. }) => {
                        // Nothing to undo: the game stays as it is.
                        prop_assert_eq!(undone, None);
                        prop_assert_eq!(saved(game), unchanged);
                        return Ok(());
                    }
                    Some(event) => {
                        prop_assert_eq!(undone, Some(event));
                        model.states.pop();
                        if let Some(Some(earlier)) = model.states.last() {
                            prop_assert_eq!(&saved(game), earlier, "undo gives back the game");
                        }
                        bump(&coverage.undone, 1);
                    }
                }
            }
            return Ok(());
        }
    }

    // Every change is recorded: a game that recorded nothing is unchanged.
    let now = game.history().len();
    if now == recorded {
        prop_assert_eq!(
            saved(game),
            before,
            "{:?} changed the game silently",
            action
        );
    } else {
        model.states.resize(now, None);
        model.states.push(Some(saved(game)));
    }
    Ok(())
}

/// What a game must keep true after any action.
fn check(game: &Game, model: &Model) -> Result<(), TestCaseError> {
    let data = saved(game);
    prop_assert_eq!(model.states.len(), game.history().len() + 1);

    // A saved game loads as the same game.
    let encoded = serde_json::to_string(&game.to_data()).unwrap();
    let restored = Game::from_data(serde_json::from_str(&encoded).unwrap());
    prop_assert_eq!(&saved(&restored), &data, "a saved game loads as it was");
    prop_assert_eq!(restored.legal_moves(), game.legal_moves());
    prop_assert_eq!(restored.status(), game.status());

    check_tiles(game, model)?;
    check_status(game)?;
    check_turn(game)?;
    check_legal_moves(game)?;
    check_board(game)
}

/// The game's seed and history play it again. Replaying costs a move per
/// event, so cases check it after each undo and at their end, not after
/// every action.
fn check_replay(game: &Game, model: &Model) -> Result<(), TestCaseError> {
    let replayed = Game::replay(model.seed, model.players.clone(), game.history());
    let replayed = replayed.map_err(|e| TestCaseError::fail(format!("replay: {e:?}")))?;
    prop_assert_eq!(saved(&replayed), saved(game), "replay gives the same game");
    prop_assert_eq!(replayed.legal_moves(), game.legal_moves());
    Ok(())
}

/// Every tile is in one place: on the board, in the bag, in a hand, or
/// gone with a retired player's hand. Hands hold six tiles while the bag
/// lasts; colors not playing hold none.
fn check_tiles(game: &Game, model: &Model) -> Result<(), TestCaseError> {
    let mut seen = HashSet::new();
    let board = game.board().iter().map(|m| m.tile);
    let hands = game.hands().values().flat_map(|hand| hand.iter().copied());
    for tile in board.chain(game.bag.tiles().iter().copied()).chain(hands) {
        prop_assert!(seen.insert(tile), "{:?} is in two places", tile);
    }
    prop_assert_eq!(seen.len() + model.discarded, NUMBER_OF_TILE);

    let playing = game.playing_colors();
    for (color, hand) in game.hands() {
        let expected = if !playing.contains(&color) {
            0..=0
        } else if game.bag_is_empty() {
            0..=TILES_IN_HAND
        } else {
            TILES_IN_HAND..=TILES_IN_HAND
        };
        prop_assert!(
            expected.contains(&hand.len()),
            "{:?} holds {:?}",
            color,
            hand
        );
    }

    // Each recorded move is on the board, and nothing else is.
    let mut moves = 0;
    for event in game.history() {
        if let TurnEvent::Played { m, .. } = event {
            prop_assert!(game.board().contains(m), "{:?} is not on the board", m);
            moves += 1;
        }
    }
    prop_assert_eq!(moves, game.board().len());
    Ok(())
}

/// [`Game::status`] says what passing the turn round the table finds:
/// finished once no tile is left, playing while someone can move, stalled
/// otherwise.
fn check_status(game: &Game) -> Result<(), TestCaseError> {
    let tiles_left = !game.bag_is_empty() || game.hands().values().any(|hand| !hand.is_empty());
    let mut round = game.clone();
    let mut someone_can_move = false;
    for _ in game.playing_colors() {
        if !round.legal_moves().is_empty() {
            someone_can_move = true;
            break;
        }
        round.skip_turn();
    }
    let expected = match (tiles_left, someone_can_move) {
        (false, _) => GameStatus::Finished,
        (true, true) => GameStatus::Playing,
        (true, false) => GameStatus::Stalled,
    };
    prop_assert_eq!(game.status(), expected);
    prop_assert_eq!(
        game.is_stuck(),
        expected == GameStatus::Playing && game.legal_moves().is_empty()
    );

    // Skipping stuck players reaches a player who can move, if any.
    let mut skipped = game.clone();
    skipped.skip_stuck_players();
    prop_assert_eq!(skipped.status(), expected);
    prop_assert_eq!(
        !skipped.legal_moves().is_empty(),
        expected == GameStatus::Playing
    );
    Ok(())
}

/// The turn is a playing color's, and after the free move it lasts only
/// while a forced space can be filled.
fn check_turn(game: &Game) -> Result<(), TestCaseError> {
    prop_assert!(game.playing_colors().contains(&game.current_player()));
    if game.turn_step() == TurnStep::AfterFreeMove {
        prop_assert!(!game.legal_moves().is_empty(), "a turn left open");
    }
    Ok(())
}

/// Each legal move is a tile of the current hand that keeps the golden
/// rule, and while the bag has tiles makes no same-color triple and gives
/// no forced space a fourth tile, worked out from the tiles around it.
/// Forced spaces come first, and only they after the free move.
fn check_legal_moves(game: &Game) -> Result<(), TestCaseError> {
    let tiles = tiles_of(game.board());
    let hand = game.hand(game.current_player());
    let legal = game.legal_moves();
    let any_forced = legal.iter().any(|m| m.forced);
    for m in legal {
        prop_assert!(hand.contains(&m.tile), "{:?} is not in hand", m);
        let neighbors = m.position.around().filter(|(p, _)| tiles.contains_key(p));
        prop_assert_eq!(m.forced, neighbors.count() >= 3, "{:?}", m);
        prop_assert_eq!(m.forced, any_forced, "forced spaces come first: {:?}", m);
        if game.turn_step() == TurnStep::AfterFreeMove {
            prop_assert!(m.forced, "a second free move: {:?}", m);
        }
        if let Some(rule) = broken_rule(&tiles, m) {
            prop_assert!(
                rule.is_restriction() && game.bag_is_empty(),
                "{:?} breaks {:?}, bag {}, board {:?}",
                m,
                rule,
                game.bag_count(),
                game.board()
            );
        }
    }
    Ok(())
}

/// Touching tiles show each other the same color. While the bag has tiles,
/// no empty space has a fourth tile around it, and none of three tiles
/// all showing it the same color: no legal move ever made one.
fn check_board(game: &Game) -> Result<(), TestCaseError> {
    let tiles = tiles_of(game.board());
    let mut spaces = HashSet::new();
    for m in game.board().iter() {
        for (p, dir) in m.position.around() {
            match tiles.get(&p) {
                Some(other) => prop_assert_eq!(
                    m.color(dir),
                    other.color(dir.opposite()),
                    "{:?} and {:?}",
                    m,
                    other
                ),
                None => {
                    spaces.insert(p);
                }
            }
        }
    }
    if game.bag_is_empty() {
        return Ok(());
    }
    for space in spaces {
        let colors: Vec<Color> = space
            .around()
            .filter_map(|(p, dir)| tiles.get(&p).map(|t| t.color(dir.opposite())))
            .collect();
        prop_assert!(
            colors.len() <= 3,
            "{:?} has {} tiles around it",
            space,
            colors.len()
        );
        prop_assert!(
            colors.len() < 3 || colors.iter().any(|&c| c != colors[0]),
            "{:?} is a same-color triple",
            space
        );
    }
    Ok(())
}
