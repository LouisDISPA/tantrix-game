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
//! the focus is asked of the page (a small script through
//! `document::eval`) rather than tracked here.

use std::cell::Cell;
use std::rc::Rc;

use dioxus::prelude::*;

/// What a Tab stops on.
const TABBABLE: &str = "a[href], button:not([disabled]), input:not([disabled]), \
    select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])";

/// A script on the trap `id`: `root`, and its `items` that a Tab stops on
/// and that show, in page order, before `body`.
fn script(id: &str, body: &str) -> String {
    format!(
        "const root = document.getElementById({id:?});
         const items = root
           ? [...root.querySelectorAll({TABBABLE:?})].filter((e) => e.getClientRects().length > 0)
           : [];
         {body}"
    )
}

/// Remember what has the focus, then move it in.
fn open_script(id: &str) -> String {
    script(
        id,
        &format!(
            "const saved = (window.__focusReturn ??= {{}});
             saved[{id:?}] = document.activeElement;
             (root?.querySelector('[data-autofocus]') ?? items[0])?.focus();"
        ),
    )
}

/// Give the focus back — unless the player took it elsewhere meanwhile.
fn close_script(id: &str, fallback: &[&str]) -> String {
    script(
        id,
        &format!(
            "const saved = window.__focusReturn?.[{id:?}];
             if (window.__focusReturn) delete window.__focusReturn[{id:?}];
             const shows = (e) => !!e && e.isConnected && e.getClientRects().length > 0;
             const active = document.activeElement;
             const lost = !shows(active) || active === document.body || !!root?.contains(active);
             const target = shows(saved)
               ? saved
               : {fallback:?}.map((s) => document.querySelector(s)).find(shows);
             if (lost && shows(target)) target.focus();"
        ),
    )
}

/// Focus the trap's first (`last`: last) item.
fn edge_script(id: &str, last: bool) -> String {
    script(
        id,
        &format!(
            "items[{}]?.focus();",
            if last { "items.length - 1" } else { "0" }
        ),
    )
}

fn run(js: String) {
    let _ = document::eval(&js);
}

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

    let on_change = opened.clone();
    use_effect(move || {
        let open = active() && mounted();
        if open != on_change.get() {
            on_change.set(open);
            run(if open {
                open_script(id)
            } else {
                close_script(id, fallback)
            });
        }
    });
    use_drop(move || {
        if opened.get() {
            run(close_script(id, fallback));
        }
    });

    let stop = move || if active() { "0" } else { "-1" };
    rsx! {
        span {
            class: "focus-edge",
            tabindex: stop(),
            aria_hidden: "true",
            onfocus: move |_| run(edge_script(id, true)),
        }
        div {
            id,
            class: "focus-trap",
            onmounted: move |_| mounted.set(true),
            {children}
        }
        span {
            class: "focus-edge",
            tabindex: stop(),
            aria_hidden: "true",
            onfocus: move |_| run(edge_script(id, false)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{close_script, edge_script, open_script};

    #[test]
    fn the_scripts_name_their_trap_and_fallback_as_string_literals() {
        let open = open_script("end-game-dialog");
        assert!(open.contains(r#"document.getElementById("end-game-dialog")"#));
        assert!(open.contains(r#"saved["end-game-dialog"] = document.activeElement"#));
        let close = close_script("hud-menu", &[".hud-menu-toggle", ".game-layout"]);
        assert!(close.contains(r#"[".hud-menu-toggle", ".game-layout"].map("#));
        assert!(edge_script("hud-menu", true).ends_with("items[items.length - 1]?.focus();"));
        assert!(edge_script("hud-menu", false).ends_with("items[0]?.focus();"));
    }
}
