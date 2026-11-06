//! The keyboard's focus on the game screen: how an element knows the
//! keyboard (not a pointer) focused it, and how it takes the focus a key
//! handler asked for (`state::keyboard`).

use std::rc::Rc;

use dioxus::prelude::*;

use crate::state::game_state::GameCtx;
use crate::state::ui_state::UiStateStoreExt;

/// Whether a pointer is pressing an element: a pointer focuses what it
/// presses, and only a focus without one — the keyboard's, or a key
/// handler's — previews what the element would do, as hovering it does.
/// Call [`down`](Self::down) on `pointerdown` and [`reset`](Self::reset) on
/// `blur`.
#[derive(Clone, Copy)]
pub struct PointerPress(Signal<bool>);

impl PointerPress {
    pub fn down(mut self) {
        if !*self.0.peek() {
            self.0.set(true);
        }
    }

    pub fn reset(mut self) {
        if *self.0.peek() {
            self.0.set(false);
        }
    }

    /// Whether the focus (or the click) under way comes from a pointer.
    pub fn by_pointer(self) -> bool {
        *self.0.peek()
    }
}

/// An element's [`PointerPress`]. Subscribes to nothing.
pub fn use_pointer_press() -> PointerPress {
    PointerPress(use_signal(|| false))
}

/// Give `element` (the `MountedData` its `onmounted` stores) the keyboard's
/// focus whenever `wants` (a reactive closure: is the pending request for
/// this element?) turns true, and clear the request.
pub fn use_take_focus(
    element: Signal<Option<Rc<MountedData>>>,
    wants: impl FnMut() -> bool + 'static,
) {
    let ctx = use_context::<GameCtx>();
    let wants = use_memo(wants);
    use_effect(move || {
        if !wants() {
            return;
        }
        let Some(element) = element() else { return };
        ctx.ui_state.focus_request().set(None);
        spawn(async move {
            let _ = element.set_focus(true).await;
        });
    });
}
