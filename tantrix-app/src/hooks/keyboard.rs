//! How an element of the game screen takes the keyboard's focus a key
//! handler asked for (`state::keyboard`).

use std::rc::Rc;

use dioxus::prelude::*;

use crate::state::game_state::GameCtx;
use crate::state::ui_state::UiStateStoreExt;

/// Give `element` (the `MountedData` its `onmounted` stores) the keyboard's
/// focus whenever `wants` (a reactive closure: is the pending request for
/// this element?) turns true, and clear the request.
pub(crate) fn use_take_focus(
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

/// Keys pressed while nothing has the focus (it was on something that has
/// gone: a button, a banner) go to the element `screen` selects: it takes
/// the focus and gets the key, so Escape and the arrows work whatever
/// became of the focus. Tab is left to the browser. The listener is
/// compiled `web-sys`, which the production script policy allows; only the
/// browser app has it, elsewhere this does nothing.
pub fn use_route_stray_keys(screen: &'static str) {
    stray::use_route(screen);
}

#[cfg(web_client)]
mod stray {
    use std::rc::Rc;

    use dioxus::prelude::*;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use web_sys::{Document, HtmlElement, KeyboardEvent, KeyboardEventInit};

    fn document() -> Option<Document> {
        web_sys::window()?.document()
    }

    pub fn use_route(screen: &'static str) {
        let listener = use_hook(move || {
            // `Fn`, not `FnMut`: the copy it dispatches bubbles back up to
            // the document and calls it again (to return at once).
            let route = Closure::<dyn Fn(KeyboardEvent)>::new(move |e: KeyboardEvent| {
                route(&e, screen);
            });
            if let Some(document) = document() {
                let callback: &JsValue = route.as_ref();
                let _ =
                    document.add_event_listener_with_callback("keydown", callback.unchecked_ref());
            }
            Rc::new(route)
        });
        use_drop(move || {
            if let Some(document) = document() {
                let callback: &JsValue = (*listener).as_ref();
                let _ = document
                    .remove_event_listener_with_callback("keydown", callback.unchecked_ref());
            }
        });
    }

    /// A key pressed on the page's body: the screen takes the focus, and a
    /// copy of the key, which its handlers can refuse as they would the key.
    fn route(e: &KeyboardEvent, screen: &str) {
        let Some(document) = document() else { return };
        let on_body = match (e.target(), document.body()) {
            (Some(target), Some(body)) => JsValue::from(target) == JsValue::from(body),
            _ => false,
        };
        if !on_body || e.key() == "Tab" {
            return;
        }
        let Some(screen) = document
            .query_selector(screen)
            .ok()
            .flatten()
            .and_then(|element| element.dyn_into::<HtmlElement>().ok())
        else {
            return;
        };
        let _ = screen.focus();
        let init = KeyboardEventInit::new();
        init.set_key(&e.key());
        init.set_code(&e.code());
        init.set_location(e.location());
        init.set_repeat(e.repeat());
        init.set_ctrl_key(e.ctrl_key());
        init.set_shift_key(e.shift_key());
        init.set_alt_key(e.alt_key());
        init.set_meta_key(e.meta_key());
        init.set_bubbles(true);
        init.set_cancelable(true);
        if let Ok(copy) = KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init) {
            if !screen.dispatch_event(&copy).unwrap_or(true) {
                e.prevent_default();
            }
        }
    }
}

#[cfg(not(web_client))]
mod stray {
    pub fn use_route(_screen: &'static str) {}
}
