//! A whole game: board, bag, hands, players and turn order. [`Game`] is the
//! single source of truth for a game in progress: every change goes through
//! it, and it keeps the current player's legal moves up to date. It records
//! every change as a [`TurnEvent`], so it can undo them, and a game dealt
//! from a seed can be replayed from its seed and events.

use std::sync::OnceLock;

use enum_map::{enum_map, EnumMap};
use rand::prelude::SliceRandom;
use rand::rngs::Xoshiro256PlusPlus;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::tiles::NUMBER_OF_TILE;
use crate::{
    AiView, Bag, Board, Color, EventError, Hand, Move, MoveError, Placement, PlayedMove, Player,
    PlayerScore, ReplayError, ScoringPath, SetupError, Tile, TileMoveInfo, TurnEvent, TurnStep,
    TurnTracker, TILES_IN_HAND,
};

#[derive(Debug, Clone)]
pub struct Game {
    board: Board,
    bag: Bag,
    players: EnumMap<Color, Player>,
    turn_tracker: TurnTracker,
    hands: EnumMap<Color, Hand>,
    /// The current player's legal moves at their step of the turn. Every
    /// change clears them, and the first read after it works them out once,
    /// so a game rebuilt only to be read or saved never computes them.
    legal_moves: OnceLock<Vec<Move>>,
    /// The seed the game was dealt from, when it is known.
    seed: Option<u64>,
    /// Every change since the deal, in order.
    history: Vec<TurnEvent>,
}

/// Complete server-side game state, including the hidden draw order (the
/// bag, and the seed that shuffled it). Never send this over the player
/// protocol; `tantrix_proto::GameSnapshot` omits the bag, seed and history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameData {
    board: Board,
    bag: Bag,
    turn_tracker: TurnTracker,
    hands: EnumMap<Color, Hand>,
    /// Games saved before seeds were kept have none.
    #[serde(default)]
    seed: Option<u64>,
    /// Games saved before history was kept resume with an empty one.
    #[serde(default)]
    history: Vec<TurnEvent>,
}

impl Game {
    /// A game dealt from a random seed, which it keeps (see [`Self::seed`]).
    pub fn new(
        red: Player,
        blue: Player,
        yellow: Player,
        green: Player,
    ) -> Result<Self, SetupError> {
        Self::from_players(Self::players_map(red, blue, yellow, green))
    }

    /// A game dealt from `seed`: the same seed always deals the same game.
    pub fn new_seeded(
        red: Player,
        blue: Player,
        yellow: Player,
        green: Player,
        seed: u64,
    ) -> Result<Self, SetupError> {
        Self::from_players_seeded(Self::players_map(red, blue, yellow, green), seed)
    }

    /// A game whose bag and turn order are shuffled by `rng`; a seeded `rng`
    /// always deals the same game. Its seed is unknown, so it can be undone
    /// but not replayed.
    pub fn new_with_rng<R: Rng + ?Sized>(
        red: Player,
        blue: Player,
        yellow: Player,
        green: Player,
        rng: &mut R,
    ) -> Result<Self, SetupError> {
        Self::from_players_with_rng(Self::players_map(red, blue, yellow, green), rng)
    }

    fn players_map(
        red: Player,
        blue: Player,
        yellow: Player,
        green: Player,
    ) -> EnumMap<Color, Player> {
        let mut players = enum_map! {
            _ => Player::None
        };
        players[Color::Red] = red;
        players[Color::Blue] = blue;
        players[Color::Yellow] = yellow;
        players[Color::Green] = green;
        players
    }

    /// A game between `players` dealt from a random seed.
    pub fn from_players(players: EnumMap<Color, Player>) -> Result<Self, SetupError> {
        Self::from_players_seeded(players, rand::random())
    }

    /// A game between `players` dealt from `seed`, which it keeps, so it
    /// can be replayed ([`Self::replay`]). Deals exactly like
    /// [`Self::from_players_with_rng`] with
    /// `Xoshiro256PlusPlus::seed_from_u64(seed)`.
    pub fn from_players_seeded(
        players: EnumMap<Color, Player>,
        seed: u64,
    ) -> Result<Self, SetupError> {
        let mut game =
            Self::from_players_with_rng(players, &mut Xoshiro256PlusPlus::seed_from_u64(seed))?;
        game.seed = Some(seed);
        Ok(game)
    }

    /// A game between `players` (colors set to [`Player::None`] sit out).
    /// `rng` shuffles the bag first, then the turn order, and hands are
    /// dealt in turn order, so a seeded `rng` always deals the same game.
    pub fn from_players_with_rng<R: Rng + ?Sized>(
        players: EnumMap<Color, Player>,
        rng: &mut R,
    ) -> Result<Self, SetupError> {
        let mut bag = Bag::new_with_rng(rng);

        let active_count = players.values().filter(|p| **p != Player::None).count();
        if active_count < 2 {
            return Err(SetupError::NotEnoughPlayers);
        }
        let turn_tracker = TurnTracker::new_with_rng(&players, rng)?;

        let mut hands: EnumMap<Color, Hand> = Default::default();
        for &color in turn_tracker.playing_colors() {
            hands[color].deal(&mut bag, TILES_IN_HAND);
        }

        Ok(Self::assemble(
            Board::default(),
            bag,
            players,
            turn_tracker,
            hands,
        ))
    }

    /// Capture everything needed to resume an authoritative game after a
    /// server restart, its seed and history included. Legal-move caches and
    /// AI implementations are runtime details and are deliberately excluded.
    pub fn to_data(&self) -> GameData {
        GameData {
            board: self.board.clone(),
            bag: self.bag.clone(),
            turn_tracker: self.turn_tracker.clone(),
            hands: self.hands.clone(),
            seed: self.seed,
            history: self.history.clone(),
        }
    }

    /// Restore a multiplayer game where every active color is human-owned.
    pub fn from_data(data: GameData) -> Self {
        let mut game =
            Self::with_human_players(data.board, data.bag, data.turn_tracker, data.hands);
        game.seed = data.seed;
        game.history = data.history;
        game
    }

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

    /// Mirror a game from the authoritative snapshot a server sends its
    /// clients. They never see the draw order, so the bag holds `bag_count`
    /// placeholder tiles; every playing color is human-owned.
    pub fn from_snapshot(
        board: Board,
        hands: EnumMap<Color, Hand>,
        bag_count: usize,
        current_player: Color,
        playing_colors: impl IntoIterator<Item = Color>,
        turn_step: TurnStep,
    ) -> Result<Self, SetupError> {
        let turn_tracker = TurnTracker::from_parts(current_player, playing_colors, turn_step)?;
        Ok(Self::with_human_players(
            board,
            Bag::with_count(bag_count),
            turn_tracker,
            hands,
        ))
    }

    fn with_human_players(
        board: Board,
        bag: Bag,
        turn_tracker: TurnTracker,
        hands: EnumMap<Color, Hand>,
    ) -> Self {
        let mut players = enum_map! { _ => Player::None };
        for &color in turn_tracker.playing_colors() {
            players[color] = Player::Someone;
        }
        Self::assemble(board, bag, players, turn_tracker, hands)
    }

    fn assemble(
        board: Board,
        bag: Bag,
        players: EnumMap<Color, Player>,
        turn_tracker: TurnTracker,
        hands: EnumMap<Color, Hand>,
    ) -> Self {
        Self {
            board,
            bag,
            players,
            turn_tracker,
            hands,
            legal_moves: OnceLock::new(),
            seed: None,
            history: Vec::new(),
        }
    }

    // --- Reads ---

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn hand(&self, color: Color) -> &Hand {
        &self.hands[color]
    }

    pub fn hands(&self) -> &EnumMap<Color, Hand> {
        &self.hands
    }

    pub fn player(&self, color: Color) -> &Player {
        &self.players[color]
    }

    pub fn current_player(&self) -> Color {
        self.turn_tracker.current()
    }

    pub fn turn_step(&self) -> TurnStep {
        self.turn_tracker.step()
    }

    /// The colors in the game, in turn order.
    pub fn playing_colors(&self) -> &[Color] {
        self.turn_tracker.playing_colors()
    }

    pub fn bag_count(&self) -> usize {
        self.bag.count()
    }

    /// Once the bag is empty the restriction rules no longer apply.
    pub fn bag_is_empty(&self) -> bool {
        self.bag.is_empty()
    }

    pub fn is_game_over(&self) -> bool {
        self.bag.is_empty() && self.hands.values().all(|h| h.is_empty())
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
    pub fn legal_moves(&self) -> &[Move] {
        self.legal_moves.get_or_init(|| {
            self.board.possible_moves(
                &self.hands[self.current_player()],
                self.bag.is_empty(),
                self.turn_step(),
            )
        })
    }

    /// Whether the current player has no legal move although the game is
    /// not over: [`Self::skip_stuck_players`] would pass their turn.
    pub fn is_stuck(&self) -> bool {
        !self.is_game_over() && self.legal_moves().is_empty()
    }

    /// [`Self::legal_moves`] grouped per tile of the current player's hand,
    /// in hand order.
    pub fn moves_by_tile(&self) -> Vec<TileMoveInfo> {
        let moves = self.legal_moves();
        self.hand(self.current_player())
            .iter()
            .map(|&tile| TileMoveInfo::of(tile, moves))
            .collect()
    }

    pub fn scores(&self) -> EnumMap<Color, PlayerScore> {
        self.board.scores()
    }

    pub fn scoring_paths(&self) -> EnumMap<Color, ScoringPath> {
        self.board.scoring_paths()
    }

    /// The playing colors tied for the top score on the board as it stands:
    /// the winners once the game is over, or when it is ended early.
    pub fn winners(&self) -> Vec<Color> {
        self.winners_from(&self.scores())
    }

    /// [`Self::winners`] from the board's [`Self::scores`], for a caller
    /// that has them already.
    pub fn winners_from(&self, scores: &EnumMap<Color, PlayerScore>) -> Vec<Color> {
        let colors = self.playing_colors();
        let top = colors.iter().map(|&c| scores[c].score).max().unwrap_or(0);
        colors
            .iter()
            .copied()
            .filter(|&c| scores[c].score == top)
            .collect()
    }

    /// What the current player sees of the game, as an AI is shown it.
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
    pub fn ai_choose(&self) -> Option<Move> {
        if self.is_game_over() || self.legal_moves().is_empty() {
            return None;
        }
        let Player::Ai(ai) = &self.players[self.current_player()] else {
            return None;
        };
        Some(ai.choose_move(&self.ai_view()))
    }

    // --- Changes ---

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
            .map_err(|_| MoveError::BoardFull)?;
        let (hand_index, drawn) = self.hands[current].play_tile(canonical.tile, &mut self.bag);
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

    pub fn skip_turn(&mut self) {
        self.history.push(TurnEvent::Skipped {
            player: self.current_player(),
            step: self.turn_step(),
        });
        self.turn_tracker.next();
        self.clear_legal_moves();
    }

    /// Pass the turn over players who have no legal move, checking each
    /// player at most once. Returns `true` when none of them can move although
    /// the game is not over: it has stalled, with tiles left that fit nowhere.
    /// A game that is over is left as it is.
    pub fn skip_stuck_players(&mut self) -> bool {
        for _ in 0..self.playing_colors().len() {
            if !self.is_stuck() {
                return false;
            }
            self.skip_turn();
        }
        true
    }

    /// Play all AI turns and skip players with no valid moves. Returns when
    /// it is a human player's turn, the game is over, or no one can move —
    /// `true` in that last case, like [`Game::skip_stuck_players`].
    pub fn advance(&mut self) -> bool {
        while !self.skip_stuck_players() {
            let Some(m) = self.ai_choose() else {
                return false; // Game over, or a human player with moves
            };
            self.play_move(m.placement())
                .expect("AI chose invalid move");
        }
        true
    }

    /// Remove `color` from the turn cycle mid-game. Used when a seated
    /// participant is kicked: their seat and hand go away, and the cycle
    /// rotates past them cleanly.
    ///
    /// Returns `Err` if the color wasn't currently playing, or if removing
    /// it would leave fewer than one player in the cycle.
    pub fn retire_color(&mut self, color: Color) -> Result<(), &'static str> {
        self.turn_tracker.retire(color)?;
        self.players[color] = Player::None;
        self.hands[color] = Hand::default();
        self.history.push(TurnEvent::Retired { color });
        self.clear_legal_moves();
        Ok(())
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

    /// Forget the legal moves after a change: the next read works them out
    /// for the new state.
    fn clear_legal_moves(&mut self) {
        self.legal_moves = OnceLock::new();
    }
}

impl AiView<'_> {
    /// The tiles neither on the board nor in a hand: the bag's, as far as
    /// the AI can tell (a retired player's tiles are out of the game too).
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
    use enum_map::EnumMap;
    use rand::rngs::Xoshiro256PlusPlus;
    use rand::SeedableRng;

    use crate::test_games::for_each_seeded_state;
    use crate::{
        AiView, Bag, Board, Color, Direction, EventError, Game, Hand, Move, MoveError, Placement,
        Player, Position, ReplayError, Rotation, SetupError, TantrixAi, Tile, TurnEvent, TurnStep,
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
    fn persisted_game_keeps_bag_and_turn() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        let first = game.legal_moves()[0];
        game.play_move(first.placement()).unwrap();

        let encoded = serde_json::to_vec(&game.to_data()).unwrap();
        let restored = Game::from_data(serde_json::from_slice(&encoded).unwrap());
        assert_eq!(restored.board(), game.board());
        assert_eq!(restored.bag_count(), game.bag_count());
        assert_eq!(restored.current_player(), game.current_player());
        for &color in game.playing_colors() {
            assert_eq!(restored.hand(color), game.hand(color));
        }
        assert!(!encoded.is_empty());
    }

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
    fn game_new_rejects_less_than_two_players() {
        assert_eq!(
            Game::new(Player::Someone, Player::None, Player::None, Player::None).unwrap_err(),
            SetupError::NotEnoughPlayers
        );
    }

    #[test]
    fn a_players_map_deals_like_one_player_per_color() {
        let players = EnumMap::from_fn(|c| match c {
            Color::Blue | Color::Green => Player::Someone,
            Color::Red | Color::Yellow => Player::None,
        });
        let from_map =
            Game::from_players_with_rng(players, &mut Xoshiro256PlusPlus::seed_from_u64(3))
                .unwrap();
        let from_args = Game::new_with_rng(
            Player::None,
            Player::Someone,
            Player::None,
            Player::Someone,
            &mut Xoshiro256PlusPlus::seed_from_u64(3),
        )
        .unwrap();
        assert_eq!(from_map.playing_colors(), from_args.playing_colors());
        assert_eq!(from_map.hands(), from_args.hands());
        assert_eq!(from_map.bag_count(), from_args.bag_count());
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
        assert!(game.retire_color(last).is_err());
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
        assert!(game.retire_color(absent).is_err());
    }

    // --- Legal moves per hand tile ---

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

    // --- Mirroring a server snapshot ---

    #[test]
    fn a_snapshot_mirrors_the_game() {
        let game = seeded_game_where(after_free_move);
        let mirror = Game::from_snapshot(
            game.board().clone(),
            game.hands().clone(),
            game.bag_count(),
            game.current_player(),
            game.playing_colors().iter().copied(),
            game.turn_step(),
        )
        .unwrap();

        assert_eq!(mirror.board(), game.board());
        assert_eq!(mirror.hands(), game.hands());
        assert_eq!(mirror.bag_count(), game.bag_count());
        assert_eq!(mirror.current_player(), game.current_player());
        assert_eq!(mirror.playing_colors(), game.playing_colors());
        assert_eq!(mirror.turn_step(), TurnStep::AfterFreeMove);
        assert_eq!(mirror.legal_moves(), game.legal_moves());
        for &color in mirror.playing_colors() {
            assert_eq!(*mirror.player(color), Player::Someone);
        }
    }

    #[test]
    fn a_snapshot_must_name_a_playing_current_player() {
        let snapshot = Game::from_snapshot(
            Board::default(),
            EnumMap::default(),
            0,
            Color::Green,
            [Color::Red, Color::Blue],
            TurnStep::default(),
        );
        assert_eq!(snapshot.unwrap_err(), SetupError::CurrentPlayerNotInCycle);
    }

    // --- Skipping players who cannot move ---

    /// A two-player game on an empty board: Red to play, Red and Blue
    /// holding `red` and `blue` tiles, and `bag_count` tiles left to draw.
    fn game_with_hands(red: usize, blue: usize, bag_count: usize) -> Game {
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

    #[test]
    fn stuck_players_pass_the_turn_to_the_next_who_can_move() {
        let mut game = game_with_hands(0, 6, 10);
        assert!(game.legal_moves().is_empty());

        assert!(!game.skip_stuck_players());
        assert_eq!(game.current_player(), Color::Blue);
        assert!(!game.legal_moves().is_empty());

        // A player who can move keeps the turn.
        assert!(!game.skip_stuck_players());
        assert_eq!(game.current_player(), Color::Blue);
    }

    #[test]
    fn a_round_where_nobody_can_move_stalls_the_game() {
        // Tiles are left in the bag, but no one holds any.
        let mut game = game_with_hands(0, 0, 10);
        assert!(!game.is_game_over());

        assert!(game.skip_stuck_players());
        assert_eq!(game.current_player(), Color::Red);
        // `advance` gives up too instead of looping.
        game.advance();
        assert_eq!(game.current_player(), Color::Red);
    }

    #[test]
    fn a_finished_game_is_not_skipped() {
        let mut game = game_with_hands(0, 0, 0);
        assert!(game.is_game_over());
        assert!(!game.skip_stuck_players());
        assert_eq!(game.current_player(), Color::Red);
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
            loop {
                if sim.skip_stuck_players() || sim.is_game_over() {
                    break;
                }
                let m = sim.legal_moves()[0];
                sim.play_move(m.placement()).unwrap();
            }
        });
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
            let stalled = game.advance();
            assert_eq!(stalled, !game.is_game_over(), "seed {seed}");
            if stalled {
                assert!(game.skip_stuck_players(), "seed {seed}");
            }
        }
    }

    // --- Turn structure: forced spaces, one free move, forced spaces again ---

    /// A two-player game dealt from `seed`.
    fn seeded_game(seed: u64) -> Game {
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
    fn seeded_game_where(found: impl Fn(&Game) -> bool) -> Game {
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

    fn after_free_move(game: &Game) -> bool {
        game.turn_step() == TurnStep::AfterFreeMove
    }

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
            while !game.is_game_over() && !game.skip_stuck_players() {
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
    fn persisted_game_keeps_turn_step() {
        let game = seeded_game_where(after_free_move);

        let encoded = serde_json::to_vec(&game.to_data()).unwrap();
        let restored = Game::from_data(serde_json::from_slice(&encoded).unwrap());

        assert_eq!(restored.turn_step(), TurnStep::AfterFreeMove);
        assert_eq!(restored.legal_moves(), game.legal_moves());
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

    // --- History: undo and replay ---

    /// The first `count` colors, all human.
    fn players(count: usize) -> EnumMap<Color, Player> {
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
    fn state(game: &Game) -> serde_json::Value {
        serde_json::to_value(game.to_data()).unwrap()
    }

    /// Play the first legal move, skipping stuck players, until the game
    /// is over or stalls.
    fn play_out(game: &mut Game) {
        while !game.is_game_over() && !game.skip_stuck_players() {
            let m = game.legal_moves()[0];
            game.play_move(m.placement()).unwrap();
        }
    }

    #[test]
    fn a_game_keeps_the_seed_it_was_dealt_from() {
        let seeded = Game::from_players_seeded(players(2), 5).unwrap();
        let with_rng =
            Game::from_players_with_rng(players(2), &mut Xoshiro256PlusPlus::seed_from_u64(5))
                .unwrap();
        assert_eq!(seeded.seed(), Some(5));
        assert_eq!(with_rng.seed(), None);
        // Seeded deals match the seeded rng's, which `/game?seed=N` and the
        // e2e fixture were recorded with.
        let mut dealt = state(&seeded);
        dealt["seed"] = serde_json::Value::Null;
        assert_eq!(dealt, state(&with_rng));

        let random = Game::new(Player::Someone, Player::Someone, Player::None, Player::None);
        assert!(random.unwrap().seed().is_some());
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
            while !game.is_game_over() && !game.skip_stuck_players() {
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
        assert!(!game.skip_stuck_players());
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
    fn a_saved_game_keeps_its_seed_and_history() {
        let mut game = Game::from_players_seeded(players(2), 3).unwrap();
        for _ in 0..5 {
            let m = game.legal_moves()[0];
            game.play_move(m.placement()).unwrap();
        }

        let encoded = serde_json::to_vec(&game.to_data()).unwrap();
        let mut restored = Game::from_data(serde_json::from_slice(&encoded).unwrap());
        assert_eq!(restored.seed(), Some(3));
        assert_eq!(restored.history(), game.history());

        assert_eq!(restored.undo(), game.undo());
        assert_eq!(state(&restored), state(&game));
    }

    #[test]
    fn a_game_saved_before_history_loads_with_none() {
        let game = seeded_game_where(after_free_move);
        let mut saved = state(&game);
        let fields = saved.as_object_mut().unwrap();
        fields.remove("seed");
        fields.remove("history");

        let mut restored = Game::from_data(serde_json::from_value(saved).unwrap());

        assert_eq!(restored.seed(), None);
        assert!(restored.history().is_empty());
        assert_eq!(restored.board(), game.board());
        assert_eq!(restored.legal_moves(), game.legal_moves());
        assert_eq!(restored.undo(), None);
    }
}
