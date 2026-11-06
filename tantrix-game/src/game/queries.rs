//! Reading a [`Game`]: the table, the current player's legal moves, the
//! game's status, scores and winners.

use enum_map::EnumMap;

use crate::{
    Board, Color, Game, GameStatus, Hand, Move, Player, PlayerScore, ScoringPath, TileMoveInfo,
    TurnEvent, TurnStep,
};

impl Game {
    /// The tiles placed so far.
    pub fn board(&self) -> &Board {
        &self.board
    }

    /// The tiles `color` holds.
    pub fn hand(&self, color: Color) -> &Hand {
        &self.hands[color]
    }

    /// Every color's hand.
    pub fn hands(&self) -> &EnumMap<Color, Hand> {
        &self.hands
    }

    /// Who plays `color`.
    pub fn player(&self, color: Color) -> &Player {
        &self.players[color]
    }

    /// Whose turn it is.
    pub fn current_player(&self) -> Color {
        self.turn_tracker.current()
    }

    /// Where the current player is within their turn.
    pub fn turn_step(&self) -> TurnStep {
        self.turn_tracker.step()
    }

    /// The colors in the game, in turn order.
    pub fn playing_colors(&self) -> &[Color] {
        self.turn_tracker.playing_colors()
    }

    /// How many tiles are left in the bag.
    pub fn bag_count(&self) -> usize {
        self.bag.count()
    }

    /// Once the bag is empty the restriction rules no longer apply.
    pub fn bag_is_empty(&self) -> bool {
        self.bag.is_empty()
    }

    /// Whether the bag and every hand are empty: [`GameStatus::Finished`].
    /// A game can also end by stalling; see [`Self::status`].
    #[must_use]
    pub fn is_game_over(&self) -> bool {
        self.bag.is_empty() && self.hands.values().all(|h| h.is_empty())
    }

    /// Whether the game goes on, is finished, or has stalled. Skipping
    /// stuck players never changes it. Cheap while the current player can
    /// move; otherwise it works out the other players' moves.
    #[must_use]
    pub fn status(&self) -> GameStatus {
        if self.is_game_over() {
            GameStatus::Finished
        } else if self.turns_to_next_mover().is_some() {
            GameStatus::Playing
        } else {
            GameStatus::Stalled
        }
    }

    /// The seed the game was dealt from ([`Self::new`],
    /// [`Self::from_players_seeded`]…), or `None` for a game dealt by a
    /// caller's rng, mirrored from a snapshot, or saved before seeds were
    /// kept. It reveals the draw order: never send it to players.
    pub fn seed(&self) -> Option<u64> {
        self.seed
    }

    /// Every move, skipped turn and retirement since the deal, in order.
    pub fn history(&self) -> &[TurnEvent] {
        &self.history
    }

    /// The current player's legal moves at their step of the turn: only
    /// forced moves while one is possible, even after the bag is empty.
    #[must_use]
    pub fn legal_moves(&self) -> &[Move] {
        self.legal_moves.get_or_init(|| {
            self.board.possible_moves(
                &self.hands[self.current_player()],
                self.bag.is_empty(),
                self.turn_step(),
            )
        })
    }

    /// Whether the current player has no legal move but a player after
    /// them has: [`Self::skip_stuck_players`] would pass their turn.
    #[must_use]
    pub fn is_stuck(&self) -> bool {
        self.turns_to_next_mover().is_some_and(|turns| turns > 0)
    }

    /// How many turns pass before someone can move: none when the current
    /// player can, `k` when only the `k`-th player after them can at the
    /// start of their turn, `None` when nobody can (the game is finished or
    /// stalled).
    pub(super) fn turns_to_next_mover(&self) -> Option<usize> {
        if !self.legal_moves().is_empty() {
            return Some(0);
        }
        let colors = self.playing_colors();
        let current = colors.iter().position(|&c| c == self.current_player())?;
        (1..colors.len()).find(|turns| {
            let color = colors[(current + turns) % colors.len()];
            // Each turn starts where `TurnTracker::next` puts it.
            !self
                .board
                .possible_moves(&self.hands[color], self.bag.is_empty(), TurnStep::default())
                .is_empty()
        })
    }

    /// [`Self::legal_moves`] grouped per tile of the current player's hand,
    /// in hand order.
    #[must_use]
    pub fn moves_by_tile(&self) -> Vec<TileMoveInfo> {
        let moves = self.legal_moves();
        self.hand(self.current_player())
            .iter()
            .map(|&tile| TileMoveInfo::of(tile, moves))
            .collect()
    }

    /// Each color's score on the board as it stands.
    #[must_use]
    pub fn scores(&self) -> EnumMap<Color, PlayerScore> {
        self.board.scores()
    }

    /// Each color's best-scoring line or loop, with its positions.
    #[must_use]
    pub fn scoring_paths(&self) -> EnumMap<Color, ScoringPath> {
        self.board.scoring_paths()
    }

    /// The playing colors tied for the top score on the board as it stands:
    /// the winners once the game is over, or when it is ended early.
    #[must_use]
    pub fn winners(&self) -> Vec<Color> {
        self.winners_from(&self.scores())
    }

    /// [`Self::winners`] from the board's [`Self::scores`], for a caller
    /// that has them already.
    #[must_use]
    pub fn winners_from(&self, scores: &EnumMap<Color, PlayerScore>) -> Vec<Color> {
        let colors = self.playing_colors();
        let top = colors.iter().map(|&c| scores[c].score).max().unwrap_or(0);
        colors
            .iter()
            .copied()
            .filter(|&c| scores[c].score == top)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use enum_map::EnumMap;

    use crate::game::test_helpers::seeded_game;
    use crate::test_games::for_each_seeded_state;
    use crate::{
        Board, Color, Direction, Game, GameStatus, Move, Position, Rotation, Tile, TurnStep,
    };

    /// The legal moves of `game` worked out from scratch.
    fn fresh_legal_moves(game: &Game) -> Vec<Move> {
        game.board().possible_moves(
            game.hand(game.current_player()),
            game.bag_is_empty(),
            game.turn_step(),
        )
    }

    #[test]
    fn legal_moves_follow_every_change() {
        // Each state is reached by a move or a skip after its legal moves
        // were read, and undone or retired from after they were read again.
        for_each_seeded_state(6, |game| {
            assert_eq!(game.legal_moves(), fresh_legal_moves(game));
            let mut undone = game.clone();
            if undone.undo().is_some() {
                assert_eq!(undone.legal_moves(), fresh_legal_moves(&undone));
            }
            let mut retired = game.clone();
            if retired.retire_color(retired.current_player()).is_ok() {
                assert_eq!(retired.legal_moves(), fresh_legal_moves(&retired));
            }
        });
    }

    #[test]
    fn a_fresh_game_is_tied_between_every_player() {
        let game = seeded_game(0);
        assert_eq!(game.winners(), game.playing_colors());
    }

    #[test]
    fn winners_are_every_color_tied_for_the_top_score() {
        // Blue scores 2 along the Right↔Left edge; Red and Yellow score 1.
        let mut board = Board::default();
        for (tile, position) in [
            (Tile::BYYBRR, Position::CENTER),
            (Tile::BYRBRY, Position::CENTER.neighbor(Direction::Right)),
        ] {
            board
                .insert_move(Move {
                    tile,
                    position,
                    rotation: Rotation::None,
                    forced: false,
                })
                .unwrap();
        }
        let winners = |colors: [Color; 2]| {
            let hands = EnumMap::default();
            Game::from_snapshot(
                board.clone(),
                hands,
                0,
                colors[0],
                colors,
                TurnStep::default(),
            )
            .unwrap()
            .winners()
        };
        assert_eq!(winners([Color::Red, Color::Blue]), [Color::Blue]);
        assert_eq!(
            winners([Color::Red, Color::Yellow]),
            [Color::Red, Color::Yellow]
        );
    }

    #[test]
    fn moves_by_tile_groups_the_legal_moves_in_hand_order() {
        for seed in 0..5 {
            let mut game = seeded_game(seed);
            while let Some(&m) = game.legal_moves().first() {
                let hand = game.hand(game.current_player());
                let by_tile = game.moves_by_tile();
                assert_eq!(by_tile.len(), hand.len(), "seed {seed}");

                let mut grouped = Vec::new();
                for (&tile, info) in hand.iter().zip(&by_tile) {
                    let forced = game
                        .legal_moves()
                        .iter()
                        .any(|legal| legal.tile == tile && legal.forced);
                    assert_eq!(info.has_forced, forced, "seed {seed}");
                    for (&position, rotations) in &info.positions {
                        for &rotation in rotations {
                            grouped.push((tile, position, rotation));
                        }
                    }
                }
                let mut legal: Vec<_> = game
                    .legal_moves()
                    .iter()
                    .map(|m| (m.tile, m.position, m.rotation))
                    .collect();
                grouped.sort_by_key(|&(t, p, r)| (t as u8, p, r as u8));
                legal.sort_by_key(|&(t, p, r)| (t as u8, p, r as u8));
                assert_eq!(grouped, legal, "seed {seed}");

                game.play_move(m.placement()).unwrap();
            }
        }
    }

    /// Whether a player can move, found the long way: by passing the turn
    /// round the table until someone has a legal move.
    fn someone_can_move(game: &Game) -> bool {
        let mut round = game.clone();
        for _ in game.playing_colors() {
            if !round.legal_moves().is_empty() {
                return true;
            }
            round.skip_turn();
        }
        false
    }

    #[test]
    fn status_tells_who_can_still_move_without_changing_the_game() {
        for_each_seeded_state(6, |game| {
            let expected = if game.is_game_over() {
                GameStatus::Finished
            } else if someone_can_move(game) {
                GameStatus::Playing
            } else {
                GameStatus::Stalled
            };
            assert_eq!(game.status(), expected);
            assert_eq!(
                game.is_stuck(),
                expected == GameStatus::Playing && game.legal_moves().is_empty()
            );

            // Skipping stuck players never changes the status.
            let mut skipped = game.clone();
            skipped.skip_stuck_players();
            assert_eq!(skipped.status(), expected);
        });
    }
}
