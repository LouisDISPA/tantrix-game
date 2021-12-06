use rand::{prelude::SliceRandom, thread_rng};
use tantrix_game::{Game, Player};

fn main() {
    let mut game = Game::new(Player::Someone, Player::Someone, Player::None, Player::None);

    let active_color = game.currently_playing();
    let _player_tiles = game.get_hand(active_color);
    let possible_moves = game.get_possible_moves();
    let m = possible_moves[0];
    dbg!(&game);
    game.play_move(m).unwrap();

    let mut rng = thread_rng();

    for _ in 1..10 {
        let possible_moves = game.get_possible_moves();
        let pm = possible_moves.choose(&mut rng).unwrap();
        let m = *pm;
        game.play_move(m).unwrap();
        dbg!(game.board());
    }
}
