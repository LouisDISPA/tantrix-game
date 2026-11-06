//! Games built for the tests of the `game` modules, and a way to compare
//! their states.

use enum_map::EnumMap;
use rand::rngs::Xoshiro256PlusPlus;
use rand::SeedableRng;

use crate::{Bag, Board, Color, Game, Hand, Player, Tile, TurnStep, TILES_IN_HAND};

impl Game {
    /// This game with `opening` on the board instead of an empty one, for
    /// tests that play on from a hand-built position. The opening's tiles
    /// leave the bag and the hands, which refill from the bag. The seed no
    /// longer replays the deal, so the game forgets it.
    pub(crate) fn with_opening(mut self, opening: Board) -> Self {
        let placed = |tile: &Tile| opening.iter().any(|m| m.tile == *tile);
        self.bag = Bag::from_tiles(self.bag.tiles().iter().copied().filter(|t| !placed(t)));
        for &color in self.turn_tracker.playing_colors() {
            let kept: Vec<Tile> = self.hands[color]
                .iter()
                .copied()
                .filter(|t| !placed(t))
                .collect();
            let mut hand = Hand::default();
            hand.deal(&mut Bag::from_tiles(kept.iter().rev().copied()), kept.len());
            hand.deal(&mut self.bag, TILES_IN_HAND - kept.len());
            self.hands[color] = hand;
        }
        self.board = opening;
        self.seed = None;
        self.clear_legal_moves();
        self
    }
}

/// A two-player game dealt from `seed`.
pub(super) fn seeded_game(seed: u64) -> Game {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
    Game::new_with_rng(
        Player::Someone,
        Player::Someone,
        Player::None,
        Player::None,
        &mut rng,
    )
    .unwrap()
}

/// Plays the first legal move of seeded deals until `found` holds, and
/// returns that game. Deterministic: the same seeds always give the same game.
pub(super) fn seeded_game_where(found: impl Fn(&Game) -> bool) -> Game {
    for seed in 0..100 {
        let mut game = seeded_game(seed);
        loop {
            if found(&game) {
                return game;
            }
            let Some(&m) = game.legal_moves().first() else {
                break;
            };
            game.play_move(m.placement()).unwrap();
        }
    }
    panic!("no seeded game reached the wanted state");
}

pub(super) fn after_free_move(game: &Game) -> bool {
    game.turn_step() == TurnStep::AfterFreeMove
}

/// A two-player game on an empty board: Red to play, Red and Blue
/// holding `red` and `blue` tiles, and `bag_count` tiles left to draw.
pub(super) fn game_with_hands(red: usize, blue: usize, bag_count: usize) -> Game {
    let mut bag = Bag::new();
    let mut hands: EnumMap<Color, Hand> = EnumMap::default();
    hands[Color::Red].deal(&mut bag, red);
    hands[Color::Blue].deal(&mut bag, blue);
    Game::from_snapshot(
        Board::default(),
        hands,
        bag_count,
        Color::Red,
        [Color::Red, Color::Blue],
        TurnStep::default(),
    )
    .unwrap()
}

/// The first `count` colors, all human.
pub(super) fn players(count: usize) -> EnumMap<Color, Player> {
    let playing: Vec<Color> = enum_iterator::all::<Color>().take(count).collect();
    EnumMap::from_fn(|c| {
        if playing.contains(&c) {
            Player::Someone
        } else {
            Player::None
        }
    })
}

/// Everything a game holds: board, bag in draw order, hands, turn,
/// seed and history.
pub(super) fn state(game: &Game) -> serde_json::Value {
    serde_json::to_value(game.to_data()).unwrap()
}
