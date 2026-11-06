//! Undoing and replaying the [`TurnEvent`]s a [`Game`] records.

use enum_map::EnumMap;

use crate::{Color, EventError, Game, Player, ReplayError, TurnEvent, TurnStep};

impl Game {
    /// Rebuild the game dealt from `seed` between `players` by playing
    /// `events` (a [`Self::history`]) again. Only the colors playing matter
    /// for the deal; `players` may differ in who is human or AI.
    pub fn replay(
        seed: u64,
        players: EnumMap<Color, Player>,
        events: &[TurnEvent],
    ) -> Result<Self, ReplayError> {
        let mut game = Self::from_players_seeded(players, seed).map_err(ReplayError::Setup)?;
        for (index, event) in events.iter().enumerate() {
            game.replay_event(event)
                .map_err(|error| ReplayError::Event { index, error })?;
        }
        Ok(game)
    }

    /// Revert the last recorded event exactly and return it: take the tile
    /// off the board, put the drawn tiles back on top of the bag and the
    /// played tile back where it was in the hand, and give the turn back at
    /// the step it was at. `None` when there is nothing to undo, or the last
    /// event is a [`TurnEvent::Retired`], which cannot be undone.
    pub fn undo(&mut self) -> Option<TurnEvent> {
        if matches!(self.history.last()?, TurnEvent::Retired { .. }) {
            return None;
        }
        let event = self.history.pop()?;
        match &event {
            TurnEvent::Played {
                player,
                step,
                m,
                hand_index,
                drawn,
            } => {
                let removed = self.board.remove_move(&m.position);
                debug_assert_eq!(removed, Some(*m));
                self.hands[*player].unplay_tile(m.tile, *hand_index, drawn, &mut self.bag);
                self.turn_tracker.restore(*player, *step);
            }
            TurnEvent::Skipped { player, step } => self.turn_tracker.restore(*player, *step),
            TurnEvent::Retired { .. } => unreachable!("retirements are not undone"),
        }
        self.clear_legal_moves();
        Some(event)
    }

    /// Play a recorded `event` again, e.g. one [`Self::undo`] returned, and
    /// record it. It must happen here exactly as it was recorded: same
    /// player and step, and for a move the same placement, hand index and
    /// tiles drawn. Otherwise the game is left as it was.
    pub fn replay_event(&mut self, event: &TurnEvent) -> Result<(), EventError> {
        match *event {
            TurnEvent::Played {
                player, step, m, ..
            } => {
                self.check_turn(player, step)?;
                self.play_move(m.placement()).map_err(EventError::Move)?;
                if self.history.last() != Some(event) {
                    self.undo();
                    return Err(EventError::Diverged);
                }
            }
            TurnEvent::Skipped { player, step } => {
                self.check_turn(player, step)?;
                self.skip_turn();
            }
            TurnEvent::Retired { color } => self.retire_color(color).map_err(EventError::Retire)?,
        }
        Ok(())
    }

    fn check_turn(&self, player: Color, step: TurnStep) -> Result<(), EventError> {
        if self.current_player() == player && self.turn_step() == step {
            Ok(())
        } else {
            Err(EventError::WrongTurn)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::game::test_helpers::{
        after_free_move, game_with_hands, players, seeded_game, seeded_game_where, state,
    };
    use crate::{
        Color, EventError, Game, GameStatus, ReplayError, RetireError, SetupError, Tile, TurnEvent,
        TurnStep,
    };

    /// Play the first legal move, skipping stuck players, until the game
    /// is over or stalls.
    fn play_out(game: &mut Game) {
        while game.status() == GameStatus::Playing {
            game.skip_stuck_players();
            let m = game.legal_moves()[0];
            game.play_move(m.placement()).unwrap();
        }
    }

    #[test]
    fn a_move_records_its_turn_and_the_tile_it_drew() {
        let mut game = seeded_game(0);
        let player = game.current_player();
        let bag_count = game.bag_count();
        let m = game.legal_moves()[0];

        let played = game.play_move(m.placement()).unwrap();

        let [TurnEvent::Played {
            player: p,
            step,
            m: placed,
            hand_index,
            drawn,
        }] = game.history()
        else {
            panic!("one move, one event: {:?}", game.history());
        };
        assert_eq!((*p, *step), (player, TurnStep::BeforeFreeMove));
        assert_eq!((*placed, *hand_index), (played.m, played.hand_index));
        assert_eq!(drawn.as_slice(), [game.hand(player)[played.hand_index]]);
        assert_eq!(game.bag_count(), bag_count - 1);
    }

    #[test]
    fn undoing_every_event_restores_the_deal() {
        for count in 2..=4 {
            for seed in 0..5 {
                let dealt = Game::from_players_seeded(players(count), seed).unwrap();
                let mut game = dealt.clone();
                play_out(&mut game);
                let played = state(&game);
                assert!(game.history().len() > 20, "{count} players, seed {seed}");

                while game.undo().is_some() {}

                assert!(game.history().is_empty());
                assert_eq!(state(&game), state(&dealt), "{count} players, seed {seed}");
                assert_eq!(game.legal_moves(), dealt.legal_moves());
                // The bag is back in draw order: playing again draws the same tiles.
                play_out(&mut game);
                assert_eq!(state(&game), played, "{count} players, seed {seed}");
            }
        }
    }

    #[test]
    fn undo_then_the_same_move_gives_the_same_state() {
        for seed in 0..5 {
            let mut game = Game::from_players_seeded(players(3), seed).unwrap();
            while game.status() == GameStatus::Playing {
                game.skip_stuck_players();
                let (before, legal) = (state(&game), game.legal_moves().to_vec());
                let m = legal[0];
                game.play_move(m.placement()).unwrap();
                let after = state(&game);

                let undone = game.undo().expect("a move to undo");
                assert_eq!(state(&game), before, "seed {seed}");
                assert_eq!(game.legal_moves(), legal, "seed {seed}");

                game.play_move(m.placement()).unwrap();
                assert_eq!(state(&game), after, "seed {seed}");
                game.undo();
                game.replay_event(&undone).unwrap();
                assert_eq!(state(&game), after, "seed {seed}");
            }
        }
    }

    #[test]
    fn skipped_turns_are_recorded_and_undone() {
        let mut game = game_with_hands(0, 6, 10);
        game.skip_stuck_players();
        let skip = TurnEvent::Skipped {
            player: Color::Red,
            step: TurnStep::BeforeFreeMove,
        };
        assert_eq!(game.history(), std::slice::from_ref(&skip));

        assert_eq!(game.undo(), Some(skip));
        assert_eq!(game.current_player(), Color::Red);
        assert!(game.legal_moves().is_empty());
        assert_eq!(game.undo(), None);
    }

    #[test]
    fn undo_gives_back_the_turn_at_the_step_it_was_at() {
        let mut game = seeded_game_where(after_free_move);
        let player = game.current_player();
        let fill = game.legal_moves()[0];
        game.play_move(fill.placement()).unwrap();
        game.undo().unwrap();
        assert_eq!(game.current_player(), player);
        assert_eq!(game.turn_step(), TurnStep::AfterFreeMove);

        // Undoing the free move itself goes back to before it.
        let free_move = game.undo();
        assert!(matches!(free_move, Some(TurnEvent::Played { m, .. }) if !m.forced));
        assert_eq!(game.current_player(), player);
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn retiring_is_recorded_but_not_undone() {
        let mut game = Game::from_players_seeded(players(3), 1).unwrap();
        let m = game.legal_moves()[0];
        game.play_move(m.placement()).unwrap();
        let retired = game.playing_colors()[2];
        game.retire_color(retired).unwrap();
        assert_eq!(
            game.history().last(),
            Some(&TurnEvent::Retired { color: retired })
        );

        let before = state(&game);
        assert_eq!(game.undo(), None);
        assert_eq!(state(&game), before);

        let replayed = Game::replay(1, players(3), game.history()).unwrap();
        assert_eq!(state(&replayed), state(&game));
    }

    #[test]
    fn replay_rebuilds_a_seeded_game() {
        for count in 2..=4 {
            for seed in 0..5 {
                let mut game = Game::from_players_seeded(players(count), seed).unwrap();
                play_out(&mut game);

                let replayed = Game::replay(seed, players(count), game.history()).unwrap();

                assert_eq!(
                    state(&replayed),
                    state(&game),
                    "{count} players, seed {seed}"
                );
                assert_eq!(replayed.legal_moves(), game.legal_moves());
                assert_eq!(replayed.winners(), game.winners());
            }
        }
    }

    #[test]
    fn replay_refuses_events_of_another_deal() {
        let mut game = Game::from_players_seeded(players(2), 0).unwrap();
        play_out(&mut game);
        let replayed = Game::replay(1, players(2), game.history());
        assert!(
            matches!(replayed, Err(ReplayError::Event { .. })),
            "{replayed:?}"
        );
        assert!(matches!(
            Game::replay(0, players(1), game.history()),
            Err(ReplayError::Setup(SetupError::NotEnoughPlayers))
        ));
    }

    #[test]
    fn a_recorded_event_that_plays_out_otherwise_is_refused() {
        let mut game = Game::from_players_seeded(players(2), 0).unwrap();
        let m = game.legal_moves()[0];
        game.play_move(m.placement()).unwrap();
        let mut event = game.undo().unwrap();
        let before = state(&game);

        // Another tile drawn: the event is from another deal.
        let TurnEvent::Played { drawn, .. } = &mut event else {
            panic!("a move was undone");
        };
        drawn[0] = enum_iterator::all::<Tile>()
            .find(|&t| t != drawn[0])
            .unwrap();
        assert_eq!(game.replay_event(&event), Err(EventError::Diverged));
        assert_eq!(state(&game), before);

        let other = game.playing_colors()[1];
        let skip = TurnEvent::Skipped {
            player: other,
            step: TurnStep::BeforeFreeMove,
        };
        assert_eq!(game.replay_event(&skip), Err(EventError::WrongTurn));
        assert_eq!(state(&game), before);
    }

    #[test]
    fn a_recorded_retirement_of_a_color_not_playing_is_refused() {
        let mut game = Game::from_players_seeded(players(2), 0).unwrap();
        let absent = enum_iterator::all::<Color>()
            .find(|c| !game.playing_colors().contains(c))
            .unwrap();
        let before = state(&game);
        assert_eq!(
            game.replay_event(&TurnEvent::Retired { color: absent }),
            Err(EventError::Retire(RetireError::NotPlaying))
        );
        assert_eq!(state(&game), before);
    }
}
