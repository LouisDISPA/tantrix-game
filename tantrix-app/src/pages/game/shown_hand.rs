//! Whose tiles the hand panel shows.

use tantrix_game::Color;

/// The seat whose tiles the panel shows: the one the player peeked at, or
/// else their own — online their seat, locally the player to move.
///
/// A local game with AI seats must not flicker through their hands while
/// they think, so when the player to move is an AI the panel stays on the
/// next human in turn order (`turn_order`, the playing colours as they
/// take turns), the one about to play. `is_human` says whether a seat is
/// played by a person.
pub fn shown_hand(
    peeked: Option<Color>,
    current: Color,
    my_color: Option<Color>,
    turn_order: &[Color],
    is_human: impl Fn(Color) -> bool,
) -> Color {
    let own = my_color.unwrap_or_else(|| next_human(current, turn_order, &is_human));
    peeked
        .filter(|seat| turn_order.contains(seat))
        .unwrap_or(own)
}

/// `current` if a person plays it, else the first person after it in turn
/// order; `current` again when nobody does.
fn next_human(current: Color, turn_order: &[Color], is_human: &impl Fn(Color) -> bool) -> Color {
    if is_human(current) {
        return current;
    }
    let at = turn_order
        .iter()
        .position(|&seat| seat == current)
        .unwrap_or(0);
    (1..turn_order.len())
        .map(|step| turn_order[(at + step) % turn_order.len()])
        .find(|&seat| is_human(seat))
        .unwrap_or(current)
}

#[cfg(test)]
mod tests {
    use tantrix_game::Color::{Blue, Green, Red, Yellow};

    use super::shown_hand;

    const ORDER: [tantrix_game::Color; 3] = [Red, Blue, Yellow];

    #[test]
    fn a_local_game_shows_the_hand_of_the_player_to_move() {
        for current in ORDER {
            assert_eq!(shown_hand(None, current, None, &ORDER, |_| true), current);
        }
    }

    #[test]
    fn an_online_player_sees_their_own_seat_whoever_is_to_move() {
        assert_eq!(shown_hand(None, Blue, Some(Red), &ORDER, |_| true), Red);
        assert_eq!(shown_hand(None, Red, Some(Red), &ORDER, |_| true), Red);
    }

    #[test]
    fn a_watcher_follows_the_player_to_move() {
        assert_eq!(shown_hand(None, Yellow, None, &ORDER, |_| true), Yellow);
    }

    #[test]
    fn a_peek_shows_that_seat_instead() {
        assert_eq!(
            shown_hand(Some(Yellow), Red, None, &ORDER, |_| true),
            Yellow
        );
        assert_eq!(
            shown_hand(Some(Yellow), Blue, Some(Red), &ORDER, |_| true),
            Yellow
        );
        // Peeking at your own seat is just looking at it.
        assert_eq!(shown_hand(Some(Red), Red, None, &ORDER, |_| true), Red);
    }

    #[test]
    fn a_peek_at_a_seat_that_is_not_playing_is_ignored() {
        assert_eq!(shown_hand(Some(Green), Blue, None, &ORDER, |_| true), Blue);
    }

    #[test]
    fn while_an_ai_thinks_the_panel_stays_on_the_next_human() {
        // Red and Yellow are people, Blue is an AI.
        let human = |seat| seat != Blue;
        assert_eq!(shown_hand(None, Blue, None, &ORDER, human), Yellow);
        // Yellow to move: Yellow. Then Red (the turn order wraps).
        assert_eq!(shown_hand(None, Yellow, None, &ORDER, human), Yellow);
        let human = |seat| seat == Red;
        assert_eq!(shown_hand(None, Yellow, None, &ORDER, human), Red);
        assert_eq!(shown_hand(None, Blue, None, &ORDER, human), Red);
    }

    #[test]
    fn with_no_human_the_panel_follows_the_player_to_move() {
        assert_eq!(shown_hand(None, Blue, None, &ORDER, |_| false), Blue);
    }

    #[test]
    fn a_peek_beats_the_ai_rule() {
        assert_eq!(
            shown_hand(Some(Blue), Blue, None, &ORDER, |seat| seat == Red),
            Blue
        );
    }
}
