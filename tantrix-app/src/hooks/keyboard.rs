//! How an element of the game screen takes the keyboard's focus a key
//! handler asked for (`state::keyboard`).

use std::rc::Rc;

use dioxus::prelude::*;

use crate::state::game_state::GameCtx;
use crate::state::ui_state::UiStateStoreExt;

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
