//! Test-only seeded games: the e2e fixture, and positions found by playing
//! seeded games with the first legal move each turn. Seeded deals and that
//! rule make every game here the same on each run.

use serde::{Deserialize, Serialize};
use tantrix_game::{Color, Placement, Position, Rotation, Tile};

use super::config::GameConfig;
use super::game_state::GameState;

/// The seeded game that `e2e/tests/forced-after-free-move.spec.ts` plays
/// in the browser: a free move creates a forced space its player can fill,
/// so they keep the turn to fill it.
const FIXTURE: &str = include_str!("../../../e2e/fixtures/forced-after-free-move.json");

#[derive(Debug, Serialize, Deserialize)]
pub struct Fixture {
    pub seed: u64,
    pub moves: Vec<FixtureMove>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FixtureMove {
    pub player: Color,
    pub tile: Tile,
    pub position: Position,
    pub rotation: Rotation,
    pub forced: bool,
    /// Whose turn it is after this move.
    pub next: Color,
}

impl FixtureMove {
    pub fn placement(&self) -> Placement {
        Placement {
            tile: self.tile,
            position: self.position,
            rotation: self.rotation,
        }
    }
}

/// A new two-player game (Red and Blue, both human) dealt from `seed`.
pub fn seeded(seed: u64) -> GameState {
    GameState::new(&GameConfig::new(Some(seed)))
}

pub fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).unwrap()
}

/// The fixture's game after its first `moves` moves. After three, Red has
/// played their free move and must fill the forced space it made, at
/// (-1, -1), with `Tile::GYYBGB`.
pub fn fixture_after(moves: usize) -> GameState {
    let fixture = fixture();
    let mut game = seeded(fixture.seed);
    for step in &fixture.moves[..moves] {
        game.play_move(step.placement()).unwrap();
    }
    game
}

/// The first game, over seeds and then turns, in which `find` finds what
/// it looks for, with what it found. Each seeded two-player game is played
/// with the first legal move each turn.
pub fn find_game<T>(mut find: impl FnMut(&GameState) -> Option<T>) -> (GameState, T) {
    (0..100)
        .find_map(|seed| {
            let mut game = seeded(seed);
            loop {
                if let Some(found) = find(&game) {
                    return Some((game, found));
                }
                let m = *game.legal_moves().first()?;
                game.play_move(m.placement()).ok()?;
            }
        })
        .expect("a seeded game has what the test looks for")
}

/// A game under way (a tile on the board) whose player to move holds a
/// tile — at the index found — that fits at several positions, one of
/// which (found too) takes the rotations `fits` asks for.
pub fn tile_with_positions(fits: impl Fn(&[Rotation]) -> bool) -> (GameState, usize, Position) {
    let (game, (idx, at)) = find_game(|game| {
        if game.board().is_empty() {
            return None;
        }
        game.hand_info().iter().enumerate().find_map(|(idx, info)| {
            if info.positions.len() < 2 {
                return None;
            }
            let (&at, _) = info.positions.iter().find(|(_, rots)| fits(rots))?;
            Some((idx, at))
        })
    });
    (game, idx, at)
}

/// A game under way whose player to move holds a tile with exactly one
/// legal move: its index, and that move.
pub fn tile_with_one_move() -> (GameState, usize, Placement) {
    let (game, (idx, only)) = find_game(|game| {
        if game.board().is_empty() {
            return None;
        }
        let hand = game.hand(game.current_player());
        game.hand_info()
            .iter()
            .zip(hand.iter())
            .enumerate()
            .find_map(|(idx, (info, &tile))| {
                let (&position, rotations) = info.positions.iter().next()?;
                (info.positions.len() == 1 && rotations.len() == 1).then_some((
                    idx,
                    Placement {
                        tile,
                        position,
                        rotation: rotations[0],
                    },
                ))
            })
    });
    (game, idx, only)
}
