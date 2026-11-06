//! What an AI is shown of a [`Game`] ([`AiView`]), and games that go on
//! from that view for the AI to play ahead.

use rand::prelude::SliceRandom;
use rand::Rng;

use crate::tiles::NUMBER_OF_TILE;
use crate::{AiView, Bag, Game, Move, Player, Tile, TurnTracker};

impl Game {
    /// What the current player sees of the game, as an AI is shown it.
    #[must_use]
    pub fn ai_view(&self) -> AiView<'_> {
        AiView {
            board: &self.board,
            hands: &self.hands,
            current_player: self.current_player(),
            playing_colors: self.playing_colors(),
            turn_step: self.turn_step(),
            bag_count: self.bag_count(),
            legal_moves: self.legal_moves(),
        }
    }

    /// If the current player is AI, compute and return its chosen move without playing it.
    /// Returns None if the current player is human, has no moves, or the game is over.
    #[must_use]
    pub fn ai_choose(&self) -> Option<Move> {
        if self.is_game_over() || self.legal_moves().is_empty() {
            return None;
        }
        let Player::Ai(ai) = &self.players[self.current_player()] else {
            return None;
        };
        Some(ai.choose_move(&self.ai_view()))
    }
}

impl AiView<'_> {
    /// The tiles neither on the board nor in a hand: the bag's, as far as
    /// the AI can tell (a retired player's tiles are out of the game too).
    #[must_use]
    pub fn unseen_tiles(&self) -> Vec<Tile> {
        let mut seen = [false; NUMBER_OF_TILE];
        let held = self.hands.values().flat_map(|hand| hand.iter());
        for &tile in self.board.iter().map(|m| &m.tile).chain(held) {
            seen[tile as usize] = true;
        }
        enum_iterator::all::<Tile>()
            .filter(|&tile| !seen[tile as usize])
            .collect()
    }

    /// A game that goes on from this view, its bag dealt `bag_count` of the
    /// [`Self::unseen_tiles`] shuffled by `rng`: one way the draws could
    /// go, for an AI to play ahead. Every playing color is
    /// [`Player::Someone`], so the caller plays each move; the game keeps no
    /// seed, and its history starts here.
    #[must_use]
    pub fn determinize<R: Rng + ?Sized>(&self, rng: &mut R) -> Game {
        let mut unseen = self.unseen_tiles();
        unseen.shuffle(rng);
        unseen.truncate(self.bag_count);
        let turn_tracker = TurnTracker::from_parts(
            self.current_player,
            self.playing_colors.iter().copied(),
            self.turn_step,
        )
        .expect("the current player is one of the playing colors");
        Game::with_human_players(
            self.board.clone(),
            Bag::from_tiles(unseen),
            turn_tracker,
            self.hands.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::Xoshiro256PlusPlus;
    use rand::SeedableRng;

    use crate::test_games::for_each_seeded_state;
    use crate::GameStatus;

    #[test]
    fn an_ai_view_shows_the_table_but_not_the_bag() {
        for_each_seeded_state(6, |game| {
            if game.legal_moves().is_empty() {
                return;
            }
            let view = game.ai_view();
            assert_eq!(view.current_player, game.current_player());
            assert_eq!(view.turn_step, game.turn_step());
            assert_eq!(view.bag_count, game.bag_count());
            assert_eq!(view.legal_moves, game.legal_moves());

            // The unseen tiles are the bag's, in another order.
            let mut unseen = view.unseen_tiles();
            let mut bag = game.bag.tiles().to_vec();
            unseen.sort_by_key(|&t| t as usize);
            bag.sort_by_key(|&t| t as usize);
            assert_eq!(unseen, bag);
        });
    }

    #[test]
    fn a_determinized_game_goes_on_from_the_view() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
        for_each_seeded_state(6, |game| {
            if game.legal_moves().is_empty() {
                return;
            }
            let view = game.ai_view();
            let mut sim = view.determinize(&mut rng);
            assert_eq!(sim.board(), game.board());
            assert_eq!(sim.hands(), game.hands());
            assert_eq!(sim.current_player(), game.current_player());
            assert_eq!(sim.turn_step(), game.turn_step());
            assert_eq!(sim.playing_colors(), game.playing_colors());
            assert_eq!(sim.bag_count(), game.bag_count());
            assert_eq!(sim.legal_moves(), game.legal_moves());
            assert!(sim.history().is_empty() && sim.seed().is_none());

            // It plays on to the end.
            while sim.status() == GameStatus::Playing {
                sim.skip_stuck_players();
                let m = sim.legal_moves()[0];
                sim.play_move(m.placement()).unwrap();
            }
        });
    }
}
