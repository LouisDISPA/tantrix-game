use dioxus::prelude::*;
use tantrix_game::{Color, PlayerScore};

use crate::components::color_swatch::ColorSwatch;
use crate::components::tile::HandTile;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::UiStateStoreExt;
use crate::svg::paths::color_name as color_slug;

/// One player's seat: its header, which is a tab (tap it to look at that
/// player's tiles), and the tiles. The panel shows the tiles of the one
/// `shown` seat; the stylesheet lays the headers out as a row of tabs on a
/// phone, and on a wide window as the shown seat's rack along the bottom and
/// a card for every other player, a click anywhere on which looks at its
/// tiles.
///
/// What a seat's score is made of, for the line under it: its longest line,
/// or, once a loop (which counts double) outscores that, its loop. `None`
/// before the seat has built anything.
fn score_detail(score: PlayerScore) -> Option<String> {
    if score.score == 0 {
        None
    } else if score.longest_loop * 2 > score.longest_line {
        Some(format!("loop {}", score.longest_loop))
    } else {
        Some(format!("line {}", score.longest_line))
    }
}

/// Each value it shows is a memo over the game, so a move re-renders the
/// hands it changes — the mover's (a tile replaced) and those whose turn
/// starts or ends — and leaves the others alone. A peek re-renders the two
/// seats whose `shown` flag flips.
#[component]
pub fn PlayerHand(color: Color, shown: ReadSignal<Color>) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("PlayerHand", color);
    let ctx = use_context::<GameCtx>();
    let ui_state = ctx.ui_state;

    let is_current = use_memo(move || ctx.game_state.read().current_player() == color);
    let is_shown = use_memo(move || shown() == color);
    let tiles = use_memo(move || ctx.game_state.read().hand(color).to_vec());
    // Player nickname (online) + whether this seat belongs to the local user.
    let player_name = use_memo(move || ctx.driver.player_name(color));
    let is_mine = use_memo(move || ctx.driver.my_color() == Some(color));
    // What the seat's colour has built so far.
    let score = use_memo(move || ctx.game_state.read().scores()[color]);

    let color_class = format!("color-{}", color_slug(color));

    rsx! {
        div {
            class: "surface surface-blur player-hand {color_class}",
            class: if is_current() { "active" },
            class: if is_mine() { "mine" },
            class: if is_shown() { "shown" },
            // Anywhere on another seat's card looks at its tiles; the
            // header, a button, is the way in from the keyboard.
            onclick: move |_| {
                if !*is_shown.peek() {
                    ui_state.peeked().set(Some(color));
                }
            },

            button {
                class: "hand-header",
                role: "tab",
                aria_selected: "{is_shown()}",
                aria_label: match player_name() {
                    Some(name) => format!("{color}'s tiles, {name}"),
                    None => format!("{color}'s tiles"),
                },
                ColorSwatch { color }
                span { class: "hand-header-label",
                    span { class: "hand-header-color", "{color}" }
                    // Online: who sits here, and which seat is this device's.
                    if is_mine() || player_name().is_some() {
                        span { class: "hand-header-who",
                            if is_mine() {
                                span { class: "badge hand-header-you", "you" }
                            }
                            if let Some(name) = player_name() {
                                span { class: "hand-header-name", {name} }
                            }
                        }
                    }
                }
                span {
                    class: "hand-score",
                    title: "Score: the longest line, or a loop counted twice",
                    b { class: "hand-score-points", "{score().score}" }
                    if let Some(detail) = score_detail(score()) {
                        small { class: "hand-score-detail", {detail} }
                    }
                }
                if is_current() {
                    span { class: "active-dot", role: "img", aria_label: "to move" }
                }
            }

            if tiles.read().is_empty() {
                div { class: "hand-empty", "No tiles" }
            } else {
                div { class: "hand-tiles",
                    for (i, tile) in tiles.read().iter().copied().enumerate() {
                        HandTile { key: "{color:?}-{tile:?}", color, index: i }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tantrix_game::PlayerScore;

    use super::score_detail;

    fn score(longest_line: usize, longest_loop: usize) -> PlayerScore {
        PlayerScore {
            longest_line,
            longest_loop,
            score: longest_line.max(longest_loop * 2),
        }
    }

    #[test]
    fn a_score_says_whether_it_is_a_line_or_a_loop() {
        assert_eq!(score_detail(score(0, 0)), None);
        assert_eq!(score_detail(score(5, 0)).as_deref(), Some("line 5"));
        // A loop counts double: a 4-tile loop (8) beats a 5-tile line.
        assert_eq!(score_detail(score(5, 4)).as_deref(), Some("loop 4"));
        // …and a 2-tile one (4) does not.
        assert_eq!(score_detail(score(5, 2)).as_deref(), Some("line 5"));
        // On a tie the line, which it is, is named.
        assert_eq!(score_detail(score(6, 3)).as_deref(), Some("line 6"));
    }
}
