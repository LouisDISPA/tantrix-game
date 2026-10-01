//! Test-only render counter, to pin which components a state change wakes.
//!
//! A component under test records each of its renders first thing in its
//! body, behind `#[cfg(test)]`:
//!
//! ```ignore
//! #[cfg(test)]
//! crate::util::render_count::record("PlayerHand", color);
//! ```
//!
//! and a test reads the counts after driving a `VirtualDom`. Counts are per
//! thread, and each test runs on its own thread with its own `VirtualDom`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;

thread_local! {
    static RENDERS: RefCell<HashMap<(&'static str, String), usize>> = RefCell::default();
}

/// Count one render of `component`; `instance` tells its copies apart
/// (`()` for a component shown once).
pub fn record(component: &'static str, instance: impl Debug) {
    RENDERS.with(|renders| {
        *renders
            .borrow_mut()
            .entry((component, format!("{instance:?}")))
            .or_default() += 1;
    });
}

/// Renders of `component`'s copy `instance` since the last [`reset`].
pub fn of(component: &str, instance: impl Debug) -> usize {
    let instance = format!("{instance:?}");
    RENDERS.with(|renders| {
        renders
            .borrow()
            .iter()
            .find(|((name, copy), _)| *name == component && *copy == instance)
            .map_or(0, |(_, count)| *count)
    })
}

/// Renders of every copy of `component` since the last [`reset`].
pub fn total(component: &str) -> usize {
    RENDERS.with(|renders| {
        renders
            .borrow()
            .iter()
            .filter(|((name, _), _)| *name == component)
            .map(|(_, count)| count)
            .sum()
    })
}

/// Forget every count so far.
pub fn reset() {
    RENDERS.with(|renders| renders.borrow_mut().clear());
}
