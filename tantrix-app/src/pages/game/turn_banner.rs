use std::{cell::Cell, rc::Rc, time::Duration};

use dioxus::prelude::*;
use tantrix_game::{Color, Player};

use crate::state::game_state::GameCtx;
use crate::svg::paths::color_name;

#[derive(Clone, Copy, PartialEq, Eq)]
struct TurnIdentity {
    color: Color,
    here: bool,
}

fn handed_off(previous: Option<TurnIdentity>, next: TurnIdentity) -> bool {
    previous.is_some_and(|old| old.color != next.color)
}

/// A brief, non-blocking announcement. Tile selection and forced continuations
/// do not change the identity, so they cannot restart the announcement.
#[component]
pub fn TurnBanner() -> Element {
    let ctx = use_context::<GameCtx>();
    let identity = use_memo(move || {
        let game = ctx.game_state.read();
        let color = game.current_player();
        TurnIdentity {
            color,
            here: ctx.driver.controls(color) && matches!(game.player(color), Player::Someone),
        }
    });
    let previous = use_hook(|| Rc::new(Cell::new(None::<TurnIdentity>)));
    let generation = use_hook(|| Rc::new(Cell::new(0_u64)));
    let mut notice = use_signal(|| None::<(Color, String, String)>);

    use_effect(move || {
        let next = identity();
        let old = previous.replace(Some(next));
        if old == Some(next) {
            return;
        }
        let title = if next.here && !ctx.driver.runs_turns() {
            "Your turn".to_string()
        } else {
            format!("{}'s turn", next.color)
        };
        let detail = if handed_off(old, next) {
            format!("{} finished", old.unwrap().color)
        } else if next.here {
            if ctx
                .game_state
                .peek()
                .hand_info()
                .iter()
                .any(|tile| tile.has_forced)
            {
                "Fill a forced space".to_string()
            } else {
                "Select a tile".to_string()
            }
        } else {
            "Waiting for a move".to_string()
        };
        notice.set(Some((next.color, title, detail)));
        // VirtualDom state/render tests stay deterministic and need no runtime.
        if cfg!(test) {
            return;
        }
        let version = generation.get() + 1;
        generation.set(version);
        let generation = generation.clone();
        spawn(async move {
            crate::util::sleep(Duration::from_millis(1800)).await;
            if generation.get() == version {
                notice.set(None);
            }
        });
    });

    rsx! {
        div {
            class: "turn-announcement",
            role: "status",
            aria_live: "polite",
            aria_atomic: "true",
            if let Some((color, title, detail)) = notice() {
                div {
                    key: "{color:?}-{title}",
                    class: "surface surface-raised surface-blur banner turn-banner color-{color_name(color)}",
                    span {
                        class: "color-swatch banner-icon",
                        aria_hidden: "true",
                    }
                    div {
                        strong { class: "banner-title", {title} }
                        span { class: "banner-detail", {detail} }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_continuations_and_claiming_a_seat_are_not_handoffs() {
        let red = TurnIdentity {
            color: Color::Red,
            here: true,
        };
        assert!(!handed_off(None, red));
        assert!(!handed_off(Some(red), red));
        assert!(!handed_off(Some(TurnIdentity { here: false, ..red }), red));
        assert!(handed_off(
            Some(red),
            TurnIdentity {
                color: Color::Blue,
                here: true
            }
        ));
    }
}
