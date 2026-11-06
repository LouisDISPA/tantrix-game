//! Changing a [`Game`]: moves, skipped turns, AI turns and retirements,
//! each recorded as a [`TurnEvent`].

use crate::{
    Color, Game, GameStatus, Hand, MoveError, Placement, PlayedMove, Player, RetireError,
    TurnEvent, TurnStep,
};

impl Game {
    /// Play `placement` for the current player, draw a replacement when available, and
    /// pass the turn once it is over. `placement` must match one of the
    /// [`Self::legal_moves`]; the returned [`PlayedMove`] holds that move as
    /// placed, with the board's `forced` flag.
    ///
    /// Fill forced spaces, make one free move, then fill forced spaces again.
    /// These steps still apply with an empty bag; only the three restriction
    /// rules are lifted then.
    pub fn play_move(&mut self, placement: Placement) -> Result<PlayedMove, MoveError> {
        let current = self.current_player();
        let step = self.turn_step();
        if !self.hands[current].contains(&placement.tile) {
            return Err(MoveError::TileNotInHand);
        }
        if self.legal_moves().is_empty() {
            return Err(MoveError::NoLegalMoves);
        }
        let Some(&canonical) = self
            .legal_moves()
            .iter()
            .find(|legal| legal.placement() == placement)
        else {
            return Err(MoveError::InvalidMove);
        };

        self.board
            .insert_move(canonical)
            .map_err(MoveError::Board)?;
        // The tile is in hand (checked above), so this does not fail.
        let (hand_index, drawn) = self.hands[current].play_tile(canonical.tile, &mut self.bag)?;
        self.history.push(TurnEvent::Played {
            player: current,
            step,
            m: canonical,
            hand_index,
            drawn,
        });

        // The one move of a turn that isn't in a forced space is the free move.
        if !canonical.forced {
            self.turn_tracker.free_move_played();
        }
        self.clear_legal_moves();
        // The turn is over once the free move is played and the player can
        // fill no more forced spaces.
        if self.turn_step() == TurnStep::AfterFreeMove && self.legal_moves().is_empty() {
            self.turn_tracker.next();
            self.clear_legal_moves();
        }

        Ok(PlayedMove {
            m: canonical,
            hand_index,
        })
    }

    /// End the current player's turn without playing, and record it.
    pub fn skip_turn(&mut self) {
        self.history.push(TurnEvent::Skipped {
            player: self.current_player(),
            step: self.turn_step(),
        });
        self.turn_tracker.next();
        self.clear_legal_moves();
    }

    /// Pass the turn over the players who have no legal move, recording a
    /// skip for each, to the next player who can move. A game that is not
    /// [`GameStatus::Playing`] is left as it is, its history untouched:
    /// once finished or stalled, nobody can move, so nobody is skipped.
    pub fn skip_stuck_players(&mut self) {
        for _ in 0..self.turns_to_next_mover().unwrap_or(0) {
            self.skip_turn();
        }
    }

    /// Play all AI turns and skip players with no legal move. Returns when
    /// it is a human player's turn or the game is no longer
    /// [`GameStatus::Playing`]; [`Self::status`] tells which.
    ///
    /// Stops at an AI move that [`Self::play_move`] refuses and returns its
    /// error: the moves before it stay played, and that AI is still to play.
    pub fn advance(&mut self) -> Result<(), MoveError> {
        while self.status() == GameStatus::Playing {
            self.skip_stuck_players();
            let Some(m) = self.ai_choose() else {
                return Ok(()); // A human player with moves
            };
            self.play_move(m.placement())?;
        }
        Ok(())
    }

    /// Remove `color` from the turn cycle mid-game. Used when a seated
    /// participant is kicked: their seat and hand go away, and the cycle
    /// rotates past them cleanly.
    ///
    /// Fails, leaving the game as it was, if the color isn't playing
    /// ([`RetireError::NotPlaying`]) or is the last one playing
    /// ([`RetireError::LastPlayer`]).
    pub fn retire_color(&mut self, color: Color) -> Result<(), RetireError> {
        self.turn_tracker.retire(color)?;
        self.players[color] = Player::None;
        self.hands[color] = Hand::default();
        self.history.push(TurnEvent::Retired { color });
        self.clear_legal_moves();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::Xoshiro256PlusPlus;
    use rand::SeedableRng;

    use crate::game::test_helpers::{
        after_free_move, game_with_hands, seeded_game, seeded_game_where, state,
    };
    use crate::{
        AiView, Color, Game, GameStatus, Move, MoveError, Placement, Player, RetireError, Rotation,
        TantrixAi, TurnEvent, TurnStep,
    };

    #[test]
    fn a_new_game_plays_its_first_legal_move() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();

        let active_color = game.current_player();
        let _player_tiles = game.hand(active_color);
        let possible_moves = game.legal_moves();
        let m = possible_moves[0];
        game.play_move(m.placement()).unwrap();
    }

    #[test]
    fn game_play_move_rejects_illegal_rotation() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        let first = game.legal_moves()[0];
        game.play_move(first.placement()).unwrap();

        let possible = game.legal_moves().to_vec();
        let illegal = possible
            .iter()
            .find_map(|m| {
                enum_iterator::all::<Rotation>()
                    .map(|rotation| Placement {
                        rotation,
                        ..m.placement()
                    })
                    .find(|placement| !possible.iter().any(|legal| legal.placement() == *placement))
            })
            .expect("post-opening moves should include a constrained position");

        let board = game.board().clone();
        assert_eq!(game.play_move(illegal), Err(MoveError::InvalidMove));
        assert_eq!(game.board(), &board);
    }

    #[test]
    fn play_move_refuses_a_tile_not_in_hand() {
        let mut game = seeded_game(0);
        let legal = game.legal_moves()[0];
        let other = game
            .playing_colors()
            .iter()
            .copied()
            .find(|&c| c != game.current_player())
            .unwrap();
        let tile = game.hand(other)[0];

        let placement = Placement {
            tile,
            ..legal.placement()
        };
        assert_eq!(game.play_move(placement), Err(MoveError::TileNotInHand));
        assert!(game.board().is_empty());
    }

    #[test]
    fn play_move_returns_the_move_as_placed() {
        let mut game = seeded_game_where(|g| g.legal_moves().first().is_some_and(|m| m.forced));
        let player = game.current_player();
        let legal = game.legal_moves()[0];
        let index = game.hand(player).iter().position(|&t| t == legal.tile);

        // A placement says nothing about forced spaces: the board decides.
        let played = game.play_move(legal.placement()).unwrap();

        assert!(played.m.forced);
        assert_eq!(played.m, legal);
        assert_eq!(Some(played.hand_index), index);
        assert!(game.board().contains(&legal));
    }

    #[test]
    fn retire_color_drops_from_cycle() {
        let mut game = Game::new(
            Player::Someone,
            Player::Someone,
            Player::Someone,
            Player::None,
        )
        .unwrap();
        let initial = game.playing_colors().to_vec();
        assert_eq!(initial.len(), 3);
        let retire = initial[1];

        game.retire_color(retire).unwrap();

        let remaining = game.playing_colors().to_vec();
        assert_eq!(remaining.len(), 2);
        assert!(!remaining.contains(&retire));
        assert_eq!(*game.player(retire), Player::None);
        assert!(game.hand(retire).is_empty());
    }

    #[test]
    fn retire_color_advances_current_correctly() {
        // Force a deterministic cycle by retiring only colors not currently
        // in play: retiring the *current* player should roll to the next.
        let mut game = Game::new(
            Player::Someone,
            Player::Someone,
            Player::Someone,
            Player::None,
        )
        .unwrap();
        let current_before = game.current_player();
        game.retire_color(current_before).unwrap();
        let current_after = game.current_player();
        assert_ne!(current_after, current_before);
        assert!(game.playing_colors().contains(&current_after));
        assert_eq!(game.moves_by_tile().len(), game.hand(current_after).len());
    }

    #[test]
    fn retire_color_rejects_last_player() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        let first = game.playing_colors()[0];
        game.retire_color(first).unwrap();
        let last = game.playing_colors()[0];
        let before = state(&game);
        assert_eq!(game.retire_color(last), Err(RetireError::LastPlayer));
        assert_eq!(state(&game), before);
    }

    #[test]
    fn retire_color_rejects_unknown_color() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        // Find a color not in the cycle — at least one of the four must be absent.
        let absent = [Color::Red, Color::Blue, Color::Yellow, Color::Green]
            .into_iter()
            .find(|c| !game.playing_colors().contains(c))
            .expect("2-player game always has two absent colors");
        let before = state(&game);
        assert_eq!(game.retire_color(absent), Err(RetireError::NotPlaying));
        assert_eq!(state(&game), before);
    }

    // --- Skipping players who cannot move ---

    #[test]
    fn stuck_players_pass_the_turn_to_the_next_who_can_move() {
        let mut game = game_with_hands(0, 6, 10);
        assert!(game.legal_moves().is_empty());
        assert!(game.is_stuck());
        assert_eq!(game.status(), GameStatus::Playing);

        game.skip_stuck_players();
        assert_eq!(game.current_player(), Color::Blue);
        assert!(!game.legal_moves().is_empty());
        assert!(!game.is_stuck());
        assert_eq!(game.status(), GameStatus::Playing);

        // A player who can move keeps the turn.
        game.skip_stuck_players();
        assert_eq!(game.current_player(), Color::Blue);
        assert_eq!(game.history().len(), 1);
    }

    #[test]
    fn a_game_where_nobody_can_move_stalls_and_skips_nobody() {
        // Tiles are left in the bag, but no one holds any.
        let mut game = game_with_hands(0, 0, 10);
        assert!(!game.is_game_over());
        assert_eq!(game.status(), GameStatus::Stalled);
        assert!(!game.is_stuck());

        // However often it is asked, nobody is skipped and nothing is recorded.
        for _ in 0..3 {
            game.skip_stuck_players();
            assert_eq!(game.current_player(), Color::Red);
            assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
            assert!(game.history().is_empty());
            assert_eq!(game.status(), GameStatus::Stalled);
        }
        // `advance` gives up too instead of looping.
        game.advance().unwrap();
        assert_eq!(game.current_player(), Color::Red);
        assert!(game.history().is_empty());
    }

    #[test]
    fn a_finished_game_is_not_skipped() {
        let mut game = game_with_hands(0, 0, 0);
        assert!(game.is_game_over());
        assert_eq!(game.status(), GameStatus::Finished);
        game.skip_stuck_players();
        assert_eq!(game.current_player(), Color::Red);
        assert!(game.history().is_empty());
    }

    /// Always plays the first legal move.
    #[derive(Clone)]
    struct FirstMoveAi;

    impl TantrixAi for FirstMoveAi {
        fn name(&self) -> &str {
            "first"
        }

        fn clone_box(&self) -> Box<dyn TantrixAi> {
            Box::new(self.clone())
        }

        fn choose_move(&self, view: &AiView) -> Move {
            view.legal_moves[0]
        }
    }

    #[test]
    fn advance_plays_ai_turns_until_nobody_can_move() {
        for seed in 0..5 {
            let mut game = Game::new_with_rng(
                Player::Ai(Box::new(FirstMoveAi)),
                Player::None,
                Player::Ai(Box::new(FirstMoveAi)),
                Player::Ai(Box::new(FirstMoveAi)),
                &mut Xoshiro256PlusPlus::seed_from_u64(seed),
            )
            .unwrap();
            // An all-AI game only stops once it is over or nobody can move.
            game.advance().unwrap();
            assert_ne!(game.status(), GameStatus::Playing, "seed {seed}");
            let history = game.history().len();
            game.skip_stuck_players();
            assert_eq!(game.history().len(), history, "seed {seed}");
        }
    }

    /// Plays its first legal move with another player's tile.
    #[derive(Clone)]
    struct OtherTileAi;

    impl TantrixAi for OtherTileAi {
        fn name(&self) -> &str {
            "other tile"
        }

        fn clone_box(&self) -> Box<dyn TantrixAi> {
            Box::new(self.clone())
        }

        fn choose_move(&self, view: &AiView) -> Move {
            let other = view
                .playing_colors
                .iter()
                .find(|&&c| c != view.current_player)
                .unwrap();
            Move {
                tile: view.hands[*other][0],
                ..view.legal_moves[0]
            }
        }
    }

    #[test]
    fn advance_stops_at_an_ai_move_the_game_refuses() {
        let mut game = Game::new_seeded(
            Player::Ai(Box::new(OtherTileAi)),
            Player::Ai(Box::new(OtherTileAi)),
            Player::None,
            Player::None,
            0,
        )
        .unwrap();
        let (first, before) = (game.current_player(), state(&game));

        assert_eq!(game.advance(), Err(MoveError::TileNotInHand));
        assert_eq!(game.current_player(), first, "the AI is still to play");
        assert_eq!(state(&game), before);
    }

    // --- Turn structure: forced spaces, one free move, forced spaces again ---

    /// A placement the current player could make outside every forced space,
    /// respecting the restriction rules.
    fn free_placement(game: &Game) -> Option<Placement> {
        let board = game.board();
        let contour = board.contour_positions();
        let controlled = board.controlled_positions(&contour);
        game.hand(game.current_player()).iter().find_map(|&tile| {
            contour
                .iter()
                .filter(|p| !board.position_is_forced(p) && !controlled.contains(p))
                .flat_map(|&p| board.get_possible_move(tile, p))
                .find(|m| !board.creates_same_color_triple(m))
                .map(Placement::from)
        })
    }

    #[test]
    fn free_move_ends_turn_without_forced_spaces() {
        let mut game = seeded_game(0);
        let first = game.current_player();
        let opening = game.legal_moves()[0];
        assert!(!opening.forced);

        game.play_move(opening.placement()).unwrap();

        assert_ne!(game.current_player(), first);
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn forced_fill_before_free_move_keeps_the_free_move() {
        let mut game = seeded_game_where(|g| {
            g.turn_step() == TurnStep::BeforeFreeMove
                && g.legal_moves().first().is_some_and(|m| m.forced)
        });
        let player = game.current_player();

        let fill = game.legal_moves()[0];
        game.play_move(fill.placement()).unwrap();

        assert_eq!(game.current_player(), player);
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn free_move_keeps_turn_to_fill_forced_spaces() {
        let mut game = seeded_game_where(after_free_move);
        let player = game.current_player();

        // Only forced spaces are left: a second free move is refused.
        let moves = game.legal_moves().to_vec();
        assert!(!moves.is_empty());
        assert!(moves.iter().all(|m| m.forced));
        let second_free_move = free_placement(&game).expect("the hand fits a free space");
        assert_eq!(
            game.play_move(second_free_move),
            Err(MoveError::InvalidMove)
        );

        // The turn passes once no forced space can be filled.
        while game.current_player() == player {
            let fill = game.legal_moves()[0];
            assert!(fill.forced);
            game.play_move(fill.placement()).unwrap();
        }
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn empty_bag_keeps_forced_moves_before_the_free_move() {
        let source = seeded_game_where(|g| {
            g.turn_step() == TurnStep::BeforeFreeMove
                && g.legal_moves().first().is_some_and(|m| m.forced)
        });
        let geometric_forced = source.legal_moves()[0];
        let player = source.current_player();
        let mut game = Game::from_snapshot(
            source.board().clone(),
            source.hands().clone(),
            0,
            player,
            source.playing_colors().iter().copied(),
            TurnStep::BeforeFreeMove,
        )
        .unwrap();

        assert!(game.legal_moves().iter().all(|m| m.forced));
        assert!(game
            .legal_moves()
            .iter()
            .any(|m| m.placement() == geometric_forced.placement()));

        let played = game.play_move(geometric_forced.placement()).unwrap();
        assert!(played.m.forced);
        assert_eq!(game.current_player(), player);
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn drawing_the_last_tile_keeps_forced_moves() {
        let source = seeded_game_where(|g| {
            g.turn_step() == TurnStep::BeforeFreeMove
                && g.legal_moves().first().is_some_and(|m| m.forced)
        });
        let player = source.current_player();
        let mut game = Game::from_snapshot(
            source.board().clone(),
            source.hands().clone(),
            1,
            player,
            source.playing_colors().iter().copied(),
            TurnStep::BeforeFreeMove,
        )
        .unwrap();

        let fill = game.legal_moves()[0];
        assert!(game.play_move(fill.placement()).unwrap().m.forced);
        assert!(game.bag_is_empty());
        assert_eq!(game.current_player(), player);
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
        assert!(!game.legal_moves().is_empty());
        assert!(game.legal_moves().iter().all(|m| m.forced));

        let next_fill = game.legal_moves()[0];
        game.play_move(next_fill.placement()).unwrap();
        assert_eq!(game.current_player(), player);
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn a_free_move_that_empties_the_bag_still_requires_forced_fills() {
        let mut source = seeded_game_where(after_free_move);
        assert!(source.legal_moves()[0].forced);
        let TurnEvent::Played { m: free, .. } = source.undo().unwrap() else {
            panic!("the last event is a free move");
        };
        assert!(!free.forced);
        let player = source.current_player();
        let mut game = Game::from_snapshot(
            source.board().clone(),
            source.hands().clone(),
            1,
            player,
            source.playing_colors().iter().copied(),
            TurnStep::BeforeFreeMove,
        )
        .unwrap();

        game.play_move(free.placement()).unwrap();

        assert!(game.bag_is_empty());
        assert_eq!(game.current_player(), player);
        assert_eq!(game.turn_step(), TurnStep::AfterFreeMove);
        assert!(game.legal_moves().iter().all(|m| m.forced));
        assert!(!game.legal_moves().is_empty());

        while game.current_player() == player {
            let fill = game.legal_moves()[0];
            assert!(fill.forced);
            game.play_move(fill.placement()).unwrap();
        }
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn seeded_games_follow_the_three_step_turn() {
        let mut turns_kept_after_free_move = 0;
        for seed in 0..20 {
            let mut game = seeded_game(seed);
            while game.status() == GameStatus::Playing {
                game.skip_stuck_players();
                let m = game.legal_moves()[0];
                let player = game.current_player();
                let step = game.turn_step();

                game.play_move(m.placement()).unwrap();

                // After the free move, only forced spaces may be filled.
                assert!(step == TurnStep::BeforeFreeMove || m.forced, "seed {seed}");
                let free_move_done = step == TurnStep::AfterFreeMove || !m.forced;
                let forced_left = !game
                    .board()
                    .possible_moves(
                        game.hand(player),
                        game.bag_is_empty(),
                        TurnStep::AfterFreeMove,
                    )
                    .is_empty();
                // The turn passes exactly when the free move is done and no
                // forced space can be filled.
                let turn_passed = game.current_player() != player;
                assert_eq!(turn_passed, free_move_done && !forced_left, "seed {seed}");
                if free_move_done && forced_left {
                    turns_kept_after_free_move += 1;
                }
            }
        }
        assert!(turns_kept_after_free_move > 0);
    }

    #[test]
    fn next_player_starts_at_the_beginning_of_their_turn() {
        let mut skipped = seeded_game_where(after_free_move);
        skipped.skip_turn();
        assert_eq!(skipped.turn_step(), TurnStep::BeforeFreeMove);

        let mut retired = seeded_game_where(after_free_move);
        let player = retired.current_player();
        retired.retire_color(player).unwrap();
        assert_ne!(retired.current_player(), player);
        assert_eq!(retired.turn_step(), TurnStep::BeforeFreeMove);
    }
}
