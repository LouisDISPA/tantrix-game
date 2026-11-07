//! Keeps the keyboard's focus inside a dialog or an open menu, and gives it
//! back when it closes.
//!
//! While `active`, an invisible tab stop on each side of the children sends
//! a Tab that leaves them round to their other end (Shift+Tab off the first
//! to the last, Tab off the last to the first). Turning active moves the
//! focus in — to the child marked `data-autofocus`, else the first — and
//! remembers what had it; closing (turning inactive, or unmounting) gives
//! it back, or to the first of `fallback` that shows when that is gone or
//! hidden. A focus the player has taken elsewhere meanwhile is left where
//! it is.
//!
//! The children are whatever the caller puts in, so which of them can take
//! the focus is asked of the page through `web-sys` — compiled code, which
//! the production script policy allows, where evaluating JavaScript is not.
//! Only the browser app has it: elsewhere the trap renders its children and
//! no stops, and a dialog focuses its own first button.

use std::cell::Cell;
use std::rc::Rc;

use dioxus::prelude::*;

/// See the module. `id` names the children's box (one per page);
/// `fallback` lists CSS selectors, the first that shows taking the focus
/// back.
#[component]
pub fn FocusTrap(
    id: &'static str,
    active: ReadSignal<bool>,
    #[props(default)] fallback: &'static [&'static str],
    children: Element,
) -> Element {
    let mut mounted = use_signal(|| false);
    // Whether the focus was moved in (and is to be given back): not
    // reactive, read when this unmounts.
    let opened = use_hook(|| Rc::new(Cell::new(false)));
    let saved = use_hook(|| Rc::new(dom::Saved::default()));

    // What has the focus before anything moves it in: read while rendering
    // the open trap, before its children mount and one focuses itself.
    if active() {
        saved.remember();
    }

    let (on_change, saved_on_change) = (opened.clone(), saved.clone());
    use_effect(move || {
        let open = active() && mounted();
        if open != on_change.get() {
            on_change.set(open);
            if open {
                dom::focus_in(id);
            } else {
                saved_on_change.give_back(id, fallback);
            }
        }
    });
    use_drop(move || {
        if opened.get() {
            saved.give_back(id, fallback);
        }
    });

    let stop = move || if dom::TRAPS && active() { "0" } else { "-1" };
    rsx! {
        span {
            class: "focus-edge",
            tabindex: stop(),
            aria_hidden: "true",
            onfocus: move |_| dom::focus_edge(id, true),
        }
        div { id, class: "focus-trap", onmounted: move |_| mounted.set(true), {children} }
        span {
            class: "focus-edge",
            tabindex: stop(),
            aria_hidden: "true",
            onfocus: move |_| dom::focus_edge(id, false),
        }
    }
}

#[cfg(web_client)]
mod dom {
    use std::cell::{Cell, RefCell};

    use wasm_bindgen::JsCast;
    use web_sys::{Document, Element, HtmlElement};

    /// The page can be asked what takes the focus.
    pub const TRAPS: bool = true;

    /// What a Tab stops on.
    const TABBABLE: &str = "a[href], button:not([disabled]), input:not([disabled]), \
        select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])";

    fn document() -> Option<Document> {
        web_sys::window()?.document()
    }

    /// Whether `element` is on the page and shows (a hidden ancestor leaves
    /// it nothing to be laid out in).
    fn shows(element: &HtmlElement) -> bool {
        element.is_connected() && element.offset_parent().is_some()
    }

    fn html(element: Element) -> Option<HtmlElement> {
        element.dyn_into().ok()
    }

    fn root(id: &str) -> Option<Element> {
        document()?.get_element_by_id(id)
    }

    /// The trap's items a Tab stops on and that show, in page order.
    fn items(root: &Element) -> Vec<HtmlElement> {
        let Ok(list) = root.query_selector_all(TABBABLE) else {
            return Vec::new();
        };
        (0..list.length())
            .filter_map(|i| list.item(i)?.dyn_into::<HtmlElement>().ok())
            .filter(shows)
            .collect()
    }

    /// Move the focus in: to the item marked `data-autofocus`, else the first.
    pub fn focus_in(id: &str) {
        let Some(root) = root(id) else { return };
        let marked = root
            .query_selector("[data-autofocus]")
            .ok()
            .flatten()
            .and_then(html);
        if let Some(target) = marked.or_else(|| items(&root).into_iter().next()) {
            let _ = target.focus();
        }
    }

    /// Focus the trap's first (`last`: last) item.
    pub fn focus_edge(id: &str, last: bool) {
        let Some(root) = root(id) else { return };
        let items = items(&root);
        let target = if last { items.last() } else { items.first() };
        if let Some(target) = target {
            let _ = target.focus();
        }
    }

    /// What had the focus when the trap opened.
    #[derive(Default)]
    pub struct Saved {
        taken: Cell<bool>,
        element: RefCell<Option<HtmlElement>>,
    }

    impl Saved {
        /// Remember what has the focus, once per opening.
        pub fn remember(&self) {
            if !self.taken.replace(true) {
                let active = document().and_then(|d| d.active_element()).and_then(html);
                self.element.replace(active);
            }
        }

        /// Give the focus back to what had it, or to the first of
        /// `fallback` that shows — unless the player took it elsewhere.
        pub fn give_back(&self, id: &str, fallback: &[&str]) {
            if !self.taken.replace(false) {
                return;
            }
            let saved = self.element.take();
            let Some(document) = document() else { return };
            let root = root(id);
            let lost = match document.active_element().and_then(html) {
                None => true,
                Some(active) => {
                    !shows(&active)
                        || document.body().is_some_and(|body| body == active)
                        || root.is_some_and(|root| root.contains(Some(&**active)))
                }
            };
            if !lost {
                return;
            }
            let target = saved.filter(shows).or_else(|| {
                fallback
                    .iter()
                    .filter_map(|selector| document.query_selector(selector).ok().flatten())
                    .filter_map(html)
                    .find(shows)
            });
            if let Some(target) = target {
                let _ = target.focus();
            }
        }
    }
}

#[cfg(not(web_client))]
mod dom {
    /// Without `web-sys` the page cannot be asked what takes the focus.
    pub const TRAPS: bool = false;

    pub fn focus_in(_id: &str) {}

    pub fn focus_edge(_id: &str, _last: bool) {}

    #[derive(Default)]
    pub struct Saved;

    impl Saved {
        pub fn remember(&self) {}

        pub fn give_back(&self, _id: &str, _fallback: &[&str]) {}
    }
}
