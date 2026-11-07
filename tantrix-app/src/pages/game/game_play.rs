use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use tantrix_game::Player;

use super::game_hud::GameHud;
use super::player_hand::PlayerHand;
use super::shown_hand::shown_hand;
use super::tile_drag::GameDraggedTile;
use super::turn_banner::TurnBanner;
use crate::components::board::Board;
use crate::hooks::use_current_player;
use crate::state::game_state::GameCtx;
use crate::state::gesture::Pointer;
use crate::state::keyboard::{enter_turn, is_choice, step_back, Nav};
use crate::state::tile_drag::{cancel_tile_drag, drop_tile, move_tile};
use crate::state::ui_state::{set_if_changed, UiStateStoreExt};

/// Keys pressed while nothing has the focus (it was on something that has
/// gone, a button or a banner) go to the game screen: it takes the focus
/// and gets the key, so Escape and the arrows work whatever became of the
/// focus. Tab is left to the browser.
const ROUTE_STRAY_KEYS: &str = r#"
  window.__strayKeys?.();
  const stray = (e) => {
    const screen = document.querySelector(".game-layout");
    if (e.target !== document.body || e.key === "Tab" || !screen) return;
    screen.focus({ preventScroll: true });
    if (!screen.dispatchEvent(new KeyboardEvent(e.type, e))) e.preventDefault();
  };
  document.addEventListener("keydown", stray);
  window.__strayKeys = () => document.removeEventListener("keydown", stray);
"#;

const STOP_STRAY_KEYS: &str = "window.__strayKeys?.(); delete window.__strayKeys;";

/// The HUD row, the board and the hand panel. One tree for every screen:
/// the stylesheet lays it out as a column on a phone (HUD, board, panel) and
/// as a board with a side panel where there is room. Changes no state while
/// rendering: a local game's first turn is handed on by the setup screen's
/// Start Game (`actions::start_game`), an online one by the room.
///
/// `children` go in the HUD; `banners` join the centered turn announcements.
///
/// The screen holds the keyboard's focus when nothing in it does: it takes
/// it when it mounts, and keys pressed with no focus anywhere come to it
/// (`ROUTE_STRAY_KEYS`), so Escape — one step back — works from anywhere,
/// and an arrow key or Enter on it hands the focus to the turn's step
/// (`state::keyboard`). A key on it puts the player on the keys
/// (`UiState::keyboard`), a pointer pressing it takes them off. A focus
/// that lands on anything in it drops a focus a key asked for that is
/// still pending (the next turn's), so it never pulls the keyboard back
/// from where the player went.
#[component]
pub fn GamePlay(
    error: ReadSignal<Option<&'static str>>,
    #[props(default)] waiting: ReadSignal<bool>,
    #[props(default = VNode::empty())] banners: Element,
    children: Element,
) -> Element {
    #[cfg(test)]
    crate::util::render_count::record("GamePlay", ());
    let ctx = use_context::<GameCtx>();
    let ui = ctx.ui_state;
    let current = use_current_player();

    // The seats in turn order — player colors never change during a game,
    // so this memo keeps its value (and the screen its render) for the game.
    let seats = use_memo(move || ctx.game_state.read().playing_colors().to_vec());
    // Whose tiles the panel shows. Only the hands it concerns read it.
    let shown = use_memo(move || {
        let gs = ctx.game_state.read();
        shown_hand(
            ui.peeked()(),
            gs.current_player(),
            ctx.driver.my_color(),
            gs.playing_colors(),
            |seat| !matches!(gs.player(seat), Player::Ai(_)),
        )
    });
    // A peek lasts for one turn.
    use_effect(move || {
        current();
        set_if_changed(ui.peeked(), None);
    });
    // The screen itself has the focus: nothing in it has.
    let mut bare = use_signal(|| false);
    use_drop(|| {
        let _ = document::eval(STOP_STRAY_KEYS);
    });

    rsx! {
        div {
            class: "game-layout players-{seats.read().len()}",
            tabindex: "0",
            onmounted: move |e| async move {
                let _ = document::eval(ROUTE_STRAY_KEYS);
                let _ = e.data().set_focus(true).await;
            },
            onfocus: move |_| bare.set(true),
            onblur: move |_| bare.set(false),
            onfocusin: move |_| {
                if !*bare.peek() {
                    set_if_changed(ui.focus_request(), None);
                }
            },
            onpointerdown: move |evt| {
                set_if_changed(ui.keyboard(), false);
                if ui
                    .tile_drag()
                    .peek()
                    .is_some_and(|drag| {
                        drag.holds_pointer() && drag.pointer.id != evt.data().pointer_id()
                    })
                {
                    cancel_tile_drag(ctx);
                }
            },
            onpointermove: move |evt| {
                if evt.data().pointer_type() == "mouse"
                    && !evt.data().held_buttons().contains(MouseButton::Primary)
                {
                    cancel_tile_drag(ctx);
                    return;
                }
                let at = evt.data().client_coordinates();
                move_tile(ctx, Pointer::new(evt.data().pointer_id(), at.x, at.y));
            },
            onpointerup: move |evt| {
                let at = evt.data().client_coordinates();
                drop_tile(ctx, Pointer::new(evt.data().pointer_id(), at.x, at.y));
            },
            onpointercancel: move |_| cancel_tile_drag(ctx),
            onlostpointercapture: move |evt| {
                if ui
                    .tile_drag()
                    .peek()
                    .is_some_and(|drag| drag.pointer.id == evt.data().pointer_id())
                {
                    cancel_tile_drag(ctx);
                }
            },
            onpointerleave: move |_| cancel_tile_drag(ctx),
            onkeydown: move |evt: KeyboardEvent| {
                set_if_changed(ui.keyboard(), true);
                let key = evt.key();
                if key == Key::Named(NamedKey::Escape) {
                    if ui.tile_drag().peek().is_some_and(|drag| drag.holds_pointer()) {
                        cancel_tile_drag(ctx);
                    } else {
                        step_back(ctx);
                    }
                } else if *bare.peek()
                    && (is_choice(&key, evt.modifiers()) || Nav::of(&key, evt.modifiers()).is_some())
                {
                    evt.prevent_default();
                    enter_turn(ctx);
                }
            },
            GameHud { error, waiting, {children} }

            div { class: "game-announcements",
                {banners}
                TurnBanner {}
            }

            div { class: "board-slot", Board {} }

            section {
                class: "surface surface-blur hand-panel",
                role: "tablist",
                aria_label: "Players' tiles",
                for color in seats.read().iter().copied() {
                    PlayerHand { key: "{color:?}", color, shown }
                }
            }
            GameDraggedTile {}
        }
    }
}
