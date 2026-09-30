//! A whole game: board, bag, hands, players and turn order, and the one
//! checked path that plays a move.

use enum_map::{enum_map, EnumMap};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{
    Bag, Board, Color, Hand, Move, MoveError, PlayedMove, Player, PlayerScore, ScoringPath,
    SetupError, TurnStep, TurnTracker, TILES_IN_HAND,
};

/// Play `m` for the current player, draw a replacement tile, and pass the
/// turn once it is over.
///
/// A turn has three steps: fill forced spaces, make one free move, then fill
/// forced spaces again. The player keeps the turn until the free move is
/// played and no forced space is left that they can fill.
pub fn play_move_parts(
    board: &mut Board,
    hands: &mut EnumMap<Color, Hand>,
    bag: &mut Bag,
    turn: &mut TurnTracker,
    m: Move,
) -> Result<PlayedMove, MoveError> {
    let hand = &mut hands[turn.current()];
    if !hand.contains(&m.tile) {
        return Err(MoveError::TileNotInHand);
    }

    let legal_moves = board.possible_moves(hand, bag.is_empty(), turn.step());
    if legal_moves.is_empty() {
        return Err(MoveError::NoLegalMoves);
    }

    let Some(canonical) = legal_moves.into_iter().find(|legal| {
        legal.tile == m.tile && legal.position == m.position && legal.rotation == m.rotation
    }) else {
        return Err(MoveError::InvalidMove);
    };

    board
        .insert_move(canonical)
        .map_err(|_| MoveError::BoardFull)?;
    let hand_index = hand.play_tile(canonical.tile, bag);

    // The one move of a turn that isn't in a forced space is the free move.
    if !canonical.forced {
        turn.free_move_played();
    }
    if turn_is_over(board, hand, bag, turn.step()) {
        turn.next();
    }

    Ok(PlayedMove {
        m: canonical,
        hand_index,
    })
}

/// A turn is over once the free move is played and the player can fill no
/// more forced spaces.
fn turn_is_over(board: &Board, hand: &Hand, bag: &Bag, step: TurnStep) -> bool {
    step == TurnStep::AfterFreeMove && board.possible_moves(hand, bag.is_empty(), step).is_empty()
}

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
    current_moves: Option<Vec<Move>>,
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
        let mut players = enum_map! { _ => Player::None };
        for &color in data.turn_tracker.playing_colors() {
            players[color] = Player::Someone;
        }
        Self {
            board: data.board,
            bag: data.bag,
            players,
            turn_tracker: data.turn_tracker,
            hands: data.hands,
            current_moves: None,
        }
    }

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
        let board = Default::default();
        let mut bag = Bag::new_with_rng(rng);

        let mut players = enum_map! {
            _ => Player::None
        };
        players[Color::Red] = red;
        players[Color::Blue] = blue;
        players[Color::Yellow] = yellow;
        players[Color::Green] = green;

        let active_count = players.values().filter(|p| **p != Player::None).count();
        if active_count < 2 {
            return Err(SetupError::NotEnoughPlayers);
        }
        let turn_tracker = TurnTracker::new_with_rng(&players, rng)?;

        let mut hands: EnumMap<Color, Hand> = Default::default();
        for &color in turn_tracker.playing_colors() {
            hands[color].deal(&mut bag, TILES_IN_HAND);
        }

        Ok(Self {
            board,
            bag,
            players,
            turn_tracker,
            hands,
            current_moves: None,
        })
    }

    pub fn currently_playing(&self) -> Color {
        self.turn_tracker.current()
    }

    pub fn turn_step(&self) -> TurnStep {
        self.turn_tracker.step()
    }

    pub fn get_player(&self, color: Color) -> &Player {
        &self.players[color]
    }

    pub fn get_hand(&self, color: Color) -> &Hand {
        &self.hands[color]
    }

    pub fn get_possible_moves(&mut self) -> &[Move] {
        if self.current_moves.is_none() {
            let current_player = self.currently_playing();
            let moves = self.board.possible_moves(
                &self.hands[current_player],
                self.bag.is_empty(),
                self.turn_tracker.step(),
            );
            self.current_moves = Some(moves);
        }
        match &self.current_moves {
            Some(moves) => moves,
            None => unreachable!(),
        }
    }

    pub fn play_move(&mut self, m: impl Into<Move>) -> Result<(), MoveError> {
        play_move_parts(
            &mut self.board,
            &mut self.hands,
            &mut self.bag,
            &mut self.turn_tracker,
            m.into(),
        )?;
        self.current_moves = None;
        Ok(())
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn is_game_over(&self) -> bool {
        is_game_over(&self.bag, &self.hands)
    }

    pub fn bag_count(&self) -> usize {
        self.bag.count()
    }

    pub fn playing_colors(&self) -> &[Color] {
        self.turn_tracker.playing_colors()
    }

    pub fn skip_turn(&mut self) {
        self.turn_tracker.next();
        self.current_moves = None;
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
        self.current_moves = None;
        Ok(())
    }

    pub fn scores(&self) -> EnumMap<Color, PlayerScore> {
        self.board.scores()
    }

    pub fn scoring_paths(&self) -> EnumMap<Color, ScoringPath> {
        self.board.scoring_paths()
    }

    pub fn winner(&self) -> Option<(Color, PlayerScore)> {
        if !self.is_game_over() {
            return None;
        }
        let scores = self.scores();
        self.playing_colors()
            .iter()
            .map(|&c| (c, scores[c]))
            .max_by_key(|(_, s)| s.score)
    }

    /// If the current player is AI, compute and return its chosen move without playing it.
    /// Returns None if the current player is human, has no moves, or the game is over.
    pub fn ai_choose(&mut self) -> Option<Move> {
        if self.is_game_over() {
            return None;
        }
        let possible = self.get_possible_moves().to_vec();
        if possible.is_empty() {
            return None;
        }
        let current = self.currently_playing();
        if let Player::Ai(ref ai) = self.players[current] {
            let board = self.board.clone();
            Some(ai.choose_move(
                &board,
                &self.hands,
                current,
                self.turn_tracker.playing_colors(),
                &possible,
            ))
        } else {
            None
        }
    }

    /// Play all AI turns and skip players with no valid moves.
    /// Returns when it is a human player's turn or the game is over.
    pub fn advance(&mut self) {
        let num_players = self.turn_tracker.playing_colors().len();
        let mut consecutive_skips = 0;

        loop {
            if self.is_game_over() || consecutive_skips >= num_players {
                return;
            }

            if let Some(m) = self.ai_choose() {
                consecutive_skips = 0;
                self.play_move(m).expect("AI chose invalid move");
            } else if !self.get_possible_moves().is_empty() {
                return; // Human player with moves
            } else {
                self.skip_turn();
                consecutive_skips += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::Xoshiro256PlusPlus;
    use rand::SeedableRng;

    use crate::{Color, Game, Move, MoveError, Player, Rotation, TurnStep};

    #[test]
    fn persisted_game_keeps_bag_and_turn() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        let first = game.get_possible_moves()[0];
        game.play_move(first).unwrap();

        let encoded = serde_json::to_vec(&game.to_data()).unwrap();
        let restored = Game::from_data(serde_json::from_slice(&encoded).unwrap());
        assert_eq!(restored.board(), game.board());
        assert_eq!(restored.bag_count(), game.bag_count());
        assert_eq!(restored.currently_playing(), game.currently_playing());
        for &color in game.playing_colors() {
            assert_eq!(restored.get_hand(color), game.get_hand(color));
        }
        assert!(!encoded.is_empty());
    }

    #[test]
    fn a_new_game_plays_its_first_legal_move() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();

        let active_color = game.currently_playing();
        let _player_tiles = game.get_hand(active_color);
        let possible_moves = game.get_possible_moves();
        let m = possible_moves[0];
        game.play_move(m).unwrap();
    }

    #[test]
    fn game_play_move_rejects_illegal_rotation() {
        let mut game =
            Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        let first = game.get_possible_moves()[0];
        game.play_move(first).unwrap();

        let possible = game.get_possible_moves().to_vec();
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
    fn game_new_rejects_less_than_two_players() {
        assert_eq!(
            Game::new(Player::Someone, Player::None, Player::None, Player::None).unwrap_err(),
            crate::SetupError::NotEnoughPlayers
        );
    }

    #[test]
    fn test_winner_not_over() {
        let game = Game::new(Player::Someone, Player::Someone, Player::None, Player::None).unwrap();
        assert_eq!(game.winner(), None);
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
        assert_eq!(*game.get_player(retire), Player::None);
        assert!(game.get_hand(retire).is_empty());
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
        let current_before = game.currently_playing();
        game.retire_color(current_before).unwrap();
        let current_after = game.currently_playing();
        assert_ne!(current_after, current_before);
        assert!(game.playing_colors().contains(&current_after));
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
    fn seeded_game_where(found: impl Fn(&mut Game) -> bool) -> Game {
        for seed in 0..100 {
            let mut game = seeded_game(seed);
            loop {
                if found(&mut game) {
                    return game;
                }
                let Some(&m) = game.get_possible_moves().first() else {
                    break;
                };
                game.play_move(m).unwrap();
            }
        }
        panic!("no seeded game reached the wanted state");
    }

    fn after_free_move(game: &mut Game) -> bool {
        game.turn_step() == TurnStep::AfterFreeMove
    }

    /// A placement the current player could make outside every forced space,
    /// respecting the restriction rules.
    fn free_placement(game: &Game) -> Option<Move> {
        let board = game.board();
        let contour = board.contour_positions();
        let controlled = board.controlled_positions(&contour);
        game.get_hand(game.currently_playing())
            .iter()
            .find_map(|&tile| {
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
        let first = game.currently_playing();
        let opening = game.get_possible_moves()[0];
        assert!(!opening.forced);

        game.play_move(opening).unwrap();

        assert_ne!(game.currently_playing(), first);
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn forced_fill_before_free_move_keeps_the_free_move() {
        let mut game = seeded_game_where(|g| {
            g.turn_step() == TurnStep::BeforeFreeMove
                && g.get_possible_moves().first().is_some_and(|m| m.forced)
        });
        let player = game.currently_playing();

        let fill = game.get_possible_moves()[0];
        game.play_move(fill).unwrap();

        assert_eq!(game.currently_playing(), player);
        assert_eq!(game.turn_step(), TurnStep::BeforeFreeMove);
    }

    #[test]
    fn free_move_keeps_turn_to_fill_forced_spaces() {
        let mut game = seeded_game_where(after_free_move);
        let player = game.currently_playing();

        // Only forced spaces are left: a second free move is refused.
        let moves = game.get_possible_moves().to_vec();
        assert!(!moves.is_empty());
        assert!(moves.iter().all(|m| m.forced));
        let second_free_move = free_placement(&game).expect("the hand fits a free space");
        assert_eq!(
            game.play_move(second_free_move),
            Err(MoveError::InvalidMove)
        );

        // The turn passes once no forced space can be filled.
        while game.currently_playing() == player {
            let fill = game.get_possible_moves()[0];
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
            let mut stuck = 0;
            while !game.is_game_over() && stuck < game.playing_colors().len() {
                let Some(&m) = game.get_possible_moves().first() else {
                    game.skip_turn();
                    stuck += 1;
                    continue;
                };
                stuck = 0;
                let player = game.currently_playing();
                let step = game.turn_step();

                game.play_move(m).unwrap();

                // After the free move, only forced spaces may be filled.
                assert!(step == TurnStep::BeforeFreeMove || m.forced, "seed {seed}");
                let free_move_done = step == TurnStep::AfterFreeMove || !m.forced;
                let forced_left = !game
                    .board()
                    .possible_moves(
                        game.get_hand(player),
                        game.bag_count() == 0,
                        TurnStep::AfterFreeMove,
                    )
                    .is_empty();
                // The turn passes exactly when the free move is done and no
                // forced space can be filled.
                let turn_passed = game.currently_playing() != player;
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
        let mut game = seeded_game_where(after_free_move);

        let encoded = serde_json::to_vec(&game.to_data()).unwrap();
        let mut restored = Game::from_data(serde_json::from_slice(&encoded).unwrap());

        assert_eq!(restored.turn_step(), TurnStep::AfterFreeMove);
        assert_eq!(restored.get_possible_moves(), game.get_possible_moves());
    }

    #[test]
    fn next_player_starts_at_the_beginning_of_their_turn() {
        let mut skipped = seeded_game_where(after_free_move);
        skipped.skip_turn();
        assert_eq!(skipped.turn_step(), TurnStep::BeforeFreeMove);

        let mut retired = seeded_game_where(after_free_move);
        let player = retired.currently_playing();
        retired.retire_color(player).unwrap();
        assert_ne!(retired.currently_playing(), player);
        assert_eq!(retired.turn_step(), TurnStep::BeforeFreeMove);
    }
}
