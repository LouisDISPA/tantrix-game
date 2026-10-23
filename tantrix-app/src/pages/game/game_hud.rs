//! The game screen's top row: whose turn it is and what to do about it, how
//! many tiles are left in the bag, a back button while a tile is being
//! placed, the page's own badge (the online room), and the menu.
//!
//! The pill keeps the `hud-chip-*` class names the browser tests read. It
//! shows a long hint where there is room and a short one where there is not
//! (the stylesheet picks). On a wide window the stylesheet takes the hint out
//! of the pill and sets it over the shown seat's tiles, where the player is
//! looking; while another seat's tiles are peeked at, `hud-viewing` says whose
//! they are in its place.

use dioxus::prelude::*;
use tantrix_game::{Color, NUMBER_OF_TILE, TILES_IN_HAND};

use crate::components::board::{EndGameButton, ZoomControls};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::hud_menu::HudMenu;
use crate::components::theme_toggle::ThemeToggle;
use crate::hooks::{use_current_player, use_has_forced_move, use_is_game_over};
use crate::state::actions::end_game;
use crate::state::game_driver::GameDriver;
use crate::state::game_state::GameCtx;
use crate::state::ui_state::{TurnPhase, UiStateStoreExt, UiStateStoreImplExt};
use crate::svg::paths::color_name as color_slug;

/// The first sentence of a status message: what fits a narrow pill.
fn short_message(message: &str) -> &str {
    match message.find(['!', '.', '?']) {
        Some(end) => &message[..=end],
        None => message,
    }
}

/// What to do, as the pill says it: the long hint and the short one. While
/// the seat to move is played on another device (an online opponent) the
/// hint is that this device waits, whatever the phase says: it is not the
/// reader's turn to select a tile.
fn hint(
    phase: TurnPhase,
    mover: Color,
    has_forced: bool,
    played_elsewhere: bool,
) -> (String, &'static str) {
    if played_elsewhere && phase != TurnPhase::GameOver {
        return (format!("Waiting for {mover}…"), "Waiting…");
    }
    (
        phase.status_text(mover, has_forced),
        phase.short_text(has_forced),
    )
}

/// How full the bag still is, from 0 to 1: its `bag` tiles of the
/// `seats` players' deal's remainder (what the bag held to begin with).
fn bag_share(bag: usize, seats: usize) -> f64 {
    let full = NUMBER_OF_TILE.saturating_sub(seats * TILES_IN_HAND);
    if full == 0 {
        return 0.0;
    }
    (bag as f64 / full as f64).min(1.0)
}

/// Each value it shows is a memo (or the `message` field, read inline), so
/// it re-renders when its text changes: not on a hover, a drag, or an AI
/// selecting its tile. `children` is the page's own badge.
#[component]
pub fn GameHud(error: ReadSignal<Option<&'static str>>, children: Element) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("GameHud", ());
    let ctx = use_context::<GameCtx>();
    let mut ui = ctx.ui_state;

    let current_color = use_current_player();
    let has_forced = use_has_forced_move();
    let is_game_over = use_is_game_over();
    let hints = use_memo(move || {
        hint(
            ui.phase()(),
            current_color(),
            has_forced(),
            !ctx.driver.controls(current_color()),
        )
    });
    let bag_count = use_memo(move || ctx.game_state.read().bag_count());
    let bag_fill = use_memo(move || {
        let gs = ctx.game_state.read();
        bag_share(gs.bag_count(), gs.playing_colors().len())
    });
    // The seat whose tiles are being peeked at, when it is not the mover's.
    let viewing = use_memo(move || ui.peeked()().filter(|&seat| seat != current_color()));
    let can_go_back = use_memo(move || {
        matches!(
            ui.phase()(),
            TurnPhase::SelectPosition | TurnPhase::SelectRotation
        )
    });
    // The field's handle, not its value: each read below subscribes.
    let message = ui.message();
    // Local UI flag: the End Game confirmation is open.
    let mut confirming_end = use_signal(|| false);

    rsx! {
        header { class: "game-hud",
            div {
                class: "surface hud-chip color-{color_slug(current_color())}",
                class: if viewing().is_some() { "viewing" },
                class: if error().is_some() { "has-error" },
                // Left — player identity
                div { class: "hud-chip-player",
                    div { class: "color-swatch hud-chip-dot" }
                    span { class: "hud-chip-name", "{current_color}" }
                }

                span { class: "hud-chip-sep", aria_hidden: "true", "·" }

                // Center — phase instruction or alert
                if let Some(text) = error() {
                    span { class: "hud-chip-phase alert hud-chip-error", role: "status",
                        "{text}"
                    }
                } else if message.read().is_empty() {
                    span { class: "hud-chip-phase", role: "status",
                        span { class: "hud-long", "{hints().0}" }
                        span { class: "hud-short", "{hints().1}" }
                    }
                } else {
                    span { class: "hud-chip-phase alert", role: "status",
                        span { class: "hud-long", "{message}" }
                        span { class: "hud-short", "{short_message(&message.read())}" }
                    }
                }

                if let Some(seat) = viewing() {
                    span { class: "hud-viewing", "{seat}'s tiles" }
                }

                span { class: "hud-chip-sep", aria_hidden: "true", "·" }

                // Right — tiles remaining
                span { class: "hud-chip-bag", style: "--p: {bag_fill():.3}",
                    span { class: "hud-bag-count", "{bag_count}" }
                    span { class: "hud-bag-label", " in bag" }
                }
            }

            if can_go_back() {
                button {
                    class: "button button-icon button-secondary hud-back",
                    title: "Back",
                    aria_label: "Back",
                    onclick: move |_| ui.go_back(),
                    "✕"
                }
            }

            div { class: "hud-spacer" }
            {children}

            HudMenu {
                ZoomControls {}
                if !is_game_over() {
                    EndGameButton { on_click: move |_| confirming_end.set(true) }
                }
                ThemeToggle { class: "hud-menu-item game-theme-toggle", labelled: true }
            }

            if confirming_end() {
                ConfirmDialog {
                    title: "End the game now?",
                    message: match ctx.driver {
                        GameDriver::Local => "It is scored as the board stands.",
                        GameDriver::Online(_) => "It ends for everyone, scored as the board stands.",
                    },
                    confirm_label: "Yes, end it",
                    on_confirm: move |_| {
                        confirming_end.set(false);
                        end_game(ctx);
                    },
                    on_cancel: move |_| confirming_end.set(false),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tantrix_game::Color;

    use super::{bag_share, hint, short_message};
    use crate::state::ui_state::TurnPhase;

    #[test]
    fn a_narrow_pill_shows_a_messages_first_sentence() {
        assert_eq!(
            short_message("Forced move! You must fill a forced space."),
            "Forced move!"
        );
        assert_eq!(short_message("Game ended early."), "Game ended early.");
        assert_eq!(short_message("Error: no such tile"), "Error: no such tile");
        assert_eq!(short_message(""), "");
    }

    #[test]
    fn the_bag_starts_full_and_empties_to_nothing() {
        // 56 tiles, 6 to each of two players: 44 left in the bag.
        assert_eq!(bag_share(44, 2), 1.0);
        assert_eq!(bag_share(22, 2), 0.5);
        assert_eq!(bag_share(0, 2), 0.0);
        // More seats, a smaller bag to begin with.
        assert_eq!(bag_share(32, 4), 1.0);
        assert_eq!(bag_share(16, 4), 0.5);
        // Never above full, whatever the seats say.
        assert_eq!(bag_share(60, 4), 1.0);
        assert_eq!(bag_share(5, 10), 0.0);
    }

    #[test]
    fn the_hint_says_to_wait_while_another_device_plays_the_seat_to_move() {
        let (long, short) = hint(TurnPhase::SelectTile, Color::Blue, false, true);
        assert_eq!((long.as_str(), short), ("Waiting for Blue…", "Waiting…"));
        // Whatever step the remote player is at.
        assert_eq!(
            hint(TurnPhase::SelectRotation, Color::Blue, false, true).1,
            "Waiting…"
        );
        // Played here: the phase's own words.
        let (long, short) = hint(TurnPhase::SelectTile, Color::Blue, false, false);
        assert_eq!(
            (long.as_str(), short),
            ("Select a tile from your hand", "Select a tile")
        );
        assert_eq!(
            hint(TurnPhase::SelectTile, Color::Red, true, false).1,
            "Forced move!"
        );
        assert_eq!(
            hint(TurnPhase::WaitingForAi, Color::Green, false, false).0,
            "Green is thinking..."
        );
        // The end is the end for everyone.
        assert_eq!(
            hint(TurnPhase::GameOver, Color::Red, false, true).1,
            "Game over"
        );
    }
}
