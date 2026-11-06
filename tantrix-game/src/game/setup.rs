//! Dealing a [`Game`], saving and restoring it, and mirroring a server's
//! snapshot of it.

use std::sync::OnceLock;

use enum_map::{enum_map, EnumMap};
use rand::rngs::Xoshiro256PlusPlus;
use rand::{Rng, SeedableRng};

use crate::{
    Bag, Board, Color, Game, GameData, Hand, Player, SetupError, TurnStep, TurnTracker,
    TILES_IN_HAND,
};

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
    #[expect(
        clippy::disallowed_methods,
        reason = "draws a fresh seed, which the game keeps for replay"
    )]
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
    #[must_use]
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

    pub(super) fn with_human_players(
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
}

#[cfg(test)]
mod tests {
    use enum_map::EnumMap;
    use rand::rngs::Xoshiro256PlusPlus;
    use rand::SeedableRng;

    use crate::game::test_helpers::{after_free_move, players, seeded_game_where, state};
    use crate::{Board, Color, Game, Player, SetupError, TurnStep};

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

    #[test]
    fn persisted_game_keeps_turn_step() {
        let game = seeded_game_where(after_free_move);

        let encoded = serde_json::to_vec(&game.to_data()).unwrap();
        let restored = Game::from_data(serde_json::from_slice(&encoded).unwrap());

        assert_eq!(restored.turn_step(), TurnStep::AfterFreeMove);
        assert_eq!(restored.legal_moves(), game.legal_moves());
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
