//! A whole game: board, bag, hands, players and turn order. [`Game`] is the
//! single source of truth for a game in progress: every change goes through
//! it, and it keeps the current player's legal moves up to date.

use enum_map::{enum_map, EnumMap};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{
    Bag, Board, Color, Hand, Move, MoveError, PlayedMove, Player, PlayerScore, ScoringPath,
    SetupError, TileMoveInfo, TurnStep, TurnTracker, TILES_IN_HAND,
};

pub fn is_game_over(bag: &Bag, hands: &EnumMap<Color, Hand>) -> bool {
    bag.is_empty() && hands.values().all(|h| h.is_empty())
}

#[derive(Debug, Clone)]
pub struct Game {
    board: Board,
    bag: Bag,
    players: EnumMap<Color, Player>,
    turn_tracker: TurnTracker,
    hands: EnumMap<Color, Hand>,
    /// The current player's legal moves at their step of the turn,
    /// recomputed after every change so reads never compute them.
    legal_moves: Vec<Move>,
}

/// Complete server-side game state, including the hidden draw order. Never send
/// this over the player protocol; [`tantrix_proto::GameSnapshot`] omits the bag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameData {
    board: Board,
    bag: Bag,
    turn_tracker: TurnTracker,
    hands: EnumMap<Color, Hand>,
}

impl Game {
    pub fn new(
        red: Player,
        blue: Player,
        yellow: Player,
        green: Player,
    ) -> Result<Self, SetupError> {
        Self::new_with_rng(red, blue, yellow, green, &mut rand::rng())
    }

    /// A game whose bag and turn order are shuffled by `rng`; a seeded `rng`
    /// always deals the same game.
    pub fn new_with_rng<R: Rng + ?Sized>(
        red: Player,
        blue: Player,
        yellow: Player,
        green: Player,
        rng: &mut R,
    ) -> Result<Self, SetupError> {
        let mut players = enum_map! {
            _ => Player::None
        };
        players[Color::Red] = red;
        players[Color::Blue] = blue;
        players[Color::Yellow] = yellow;
        players[Color::Green] = green;
        Self::from_players_with_rng(players, rng)
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
    /// server restart. Legal-move caches and AI implementations are runtime
    /// details and are deliberately excluded.
    pub fn to_data(&self) -> GameData {
        GameData {
            board: self.board.clone(),
            bag: self.bag.clone(),
            turn_tracker: self.turn_tracker.clone(),
            hands: self.hands.clone(),
        }
    }

    /// Restore a multiplayer game where every active color is human-owned.
    pub fn from_data(data: GameData) -> Self {
        Self::with_human_players(data.board, data.bag, data.turn_tracker, data.hands)
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
        let mut game = Self {
            board,
            bag,
            players,
            turn_tracker,
            hands,
            legal_moves: Vec::new(),
        };
        game.refresh_legal_moves();
        game
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
        is_game_over(&self.bag, &self.hands)
    }

    /// The current player's legal moves at their step of the turn: only
    /// forced moves while one is possible, and after the free move.
    pub fn legal_moves(&self) -> &[Move] {
        &self.legal_moves
    }

    /// [`Self::legal_moves`] grouped per tile of the current player's hand,
    /// in hand order.
    pub fn moves_by_tile(&self) -> Vec<TileMoveInfo> {
        self.hand(self.current_player())
            .iter()
            .map(|&tile| TileMoveInfo::of(tile, &self.legal_moves))
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
        let scores = self.scores();
        let colors = self.playing_colors();
        let top = colors.iter().map(|&c| scores[c].score).max().unwrap_or(0);
        colors
            .iter()
            .copied()
            .filter(|&c| scores[c].score == top)
            .collect()
    }

    /// If the current player is AI, compute and return its chosen move without playing it.
    /// Returns None if the current player is human, has no moves, or the game is over.
    pub fn ai_choose(&self) -> Option<Move> {
        if self.is_game_over() || self.legal_moves.is_empty() {
            return None;
        }
        let current = self.current_player();
        let Player::Ai(ai) = &self.players[current] else {
            return None;
        };
        Some(ai.choose_move(
            &self.board,
            &self.hands,
            current,
            self.playing_colors(),
            &self.legal_moves,
        ))
    }

    // --- Changes ---

    /// Play `m` for the current player, draw a replacement tile, and pass the
    /// turn once it is over. `m` must match a legal move's tile, position
    /// and rotation; the returned [`PlayedMove`] holds the move as placed.
    ///
    /// A turn has three steps: fill forced spaces, make one free move, then fill
    /// forced spaces again. The player keeps the turn until the free move is
    /// played and no forced space is left that they can fill.
    pub fn play_move(&mut self, m: impl Into<Move>) -> Result<PlayedMove, MoveError> {
        let m = m.into();
        let current = self.current_player();
        if !self.hands[current].contains(&m.tile) {
            return Err(MoveError::TileNotInHand);
        }
        if self.legal_moves.is_empty() {
            return Err(MoveError::NoLegalMoves);
        }
        let Some(&canonical) = self.legal_moves.iter().find(|legal| {
            legal.tile == m.tile && legal.position == m.position && legal.rotation == m.rotation
        }) else {
            return Err(MoveError::InvalidMove);
        };

        self.board
            .insert_move(canonical)
            .map_err(|_| MoveError::BoardFull)?;
        let hand_index = self.hands[current].play_tile(canonical.tile, &mut self.bag);

        // The one move of a turn that isn't in a forced space is the free move.
        if !canonical.forced {
            self.turn_tracker.free_move_played();
        }
        self.refresh_legal_moves();
        // The turn is over once the free move is played and the player can
        // fill no more forced spaces.
        if self.turn_step() == TurnStep::AfterFreeMove && self.legal_moves.is_empty() {
            self.turn_tracker.next();
            self.refresh_legal_moves();
        }

        Ok(PlayedMove {
            m: canonical,
            hand_index,
        })
    }

    pub fn skip_turn(&mut self) {
        self.turn_tracker.next();
        self.refresh_legal_moves();
    }

    /// Pass the turn over players who have no legal move, checking each
    /// player at most once. Returns `true` when none of them can move although
    /// the game is not over: it has stalled, with tiles left that fit nowhere.
    /// A game that is over is left as it is.
    pub fn skip_stuck_players(&mut self) -> bool {
        for _ in 0..self.playing_colors().len() {
            if self.is_game_over() || !self.legal_moves.is_empty() {
                return false;
            }
            self.skip_turn();
        }
        true
    }

    /// Play all AI turns and skip players with no valid moves. Returns when
    /// it is a human player's turn, the game is over, or no one can move.
    pub fn advance(&mut self) {
        while !self.skip_stuck_players() {
            let Some(m) = self.ai_choose() else {
                return; // Game over, or a human player with moves
            };
            self.play_move(m).expect("AI chose invalid move");
        }
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
        self.refresh_legal_moves();
        Ok(())
    }

    fn refresh_legal_moves(&mut self) {
        self.legal_moves = self.board.possible_moves(
            &self.hands[self.current_player()],
            self.bag.is_empty(),
            self.turn_step(),
        );
    }
}

#[cfg(test)]
mod tests {
    use enum_map::EnumMap;
    use rand::rngs::Xoshiro256PlusPlus;
    use rand::SeedableRng;

    use crate::{
        Bag, Board, Color, Direction, Game, Hand, Move, MoveError, Player, Position, Rotation,
        SetupError, TantrixAi, Tile, TurnStep,
    };

    #[test]
    fn persisted_game_keeps_bag_and_turn() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        let first = game.legal_moves()[0];
        game.play_move(first).unwrap();

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
        game.play_move(m).unwrap();
    }

    #[test]
    fn game_play_move_rejects_illegal_rotation() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        let first = game.legal_moves()[0];
        game.play_move(first).unwrap();

        let possible = game.legal_moves().to_vec();
        let illegal = possible
            .iter()
            .find_map(|m| {
                enum_iterator::all::<Rotation>()
                    .find(|&rotation| {
                        !possible.iter().any(|legal| {
                            legal.tile == m.tile
                                && legal.position == m.position
                                && legal.rotation == rotation
                        })
                    })
                    .map(|rotation| Move { rotation, ..*m })
            })
            .expect("post-opening moves should include a constrained position");

        assert_eq!(game.play_move(illegal), Err(MoveError::InvalidMove));
    }

    #[test]
    fn play_move_returns_the_move_as_placed() {
        let mut game = seeded_game_where(|g| g.legal_moves().first().is_some_and(|m| m.forced));
        let player = game.current_player();
        let legal = game.legal_moves()[0];
        let index = game.hand(player).iter().position(|&t| t == legal.tile);

        // The board decides whether a move is forced, not the caller.
        let played = game
            .play_move(Move {
                forced: false,
                ..legal
            })
            .unwrap();

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

                game.play_move(m).unwrap();
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

        fn choose_move(
            &self,
            _board: &Board,
            _hands: &EnumMap<Color, Hand>,
            _current_player: Color,
            _active_players: &[Color],
            possible_moves: &[Move],
        ) -> Move {
            possible_moves[0]
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
            game.advance();
            assert!(
                game.is_game_over() || game.skip_stuck_players(),
                "seed {seed}"
            );
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
                game.play_move(m).unwrap();
            }
        }
        panic!("no seeded game reached the wanted state");
    }

    fn after_free_move(game: &Game) -> bool {
        game.turn_step() == TurnStep::AfterFreeMove
    }

    /// A placement the current player could make outside every forced space,
    /// respecting the restriction rules.
    fn free_placement(game: &Game) -> Option<Move> {
        let board = game.board();
        let contour = board.contour_positions();
        let controlled = board.controlled_positions(&contour);
        game.hand(game.current_player()).iter().find_map(|&tile| {
            contour
                .iter()
                .filter(|p| !board.position_is_forced(p) && !controlled.contains(p))
                .flat_map(|&p| board.get_possible_move(tile, p))
                .find(|m| !board.creates_same_color_triple(m))
        })
    }

    #[test]
    fn free_move_ends_turn_without_forced_spaces() {
        let mut game = seeded_game(0);
        let first = game.current_player();
        let opening = game.legal_moves()[0];
        assert!(!opening.forced);

        game.play_move(opening).unwrap();

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
        game.play_move(fill).unwrap();

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
            game.play_move(fill).unwrap();
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

                game.play_move(m).unwrap();

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
}
