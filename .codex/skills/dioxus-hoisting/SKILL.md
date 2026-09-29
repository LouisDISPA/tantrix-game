---
name: dioxus-hoisting
description: Dioxus 0.7 state hoisting — lifting state to a common ancestor, passing props as `ReadSignal<T>` so children participate in reactivity, and replacing mutable props with callbacks. Use when designing a component tree, choosing prop types, splitting a big component, or debugging over-/under-rendering.
allowed-tools: Read, Grep, Glob, Edit, Write
---

# Dioxus Hoisting

Source: https://dioxuslabs.com/learn/0.7/essentials/basics/hoisting

**Hoisting** = lifting shared state up to the nearest common ancestor and
passing it down as props. Paired with `ReadSignal<T>`-typed props, it
keeps re-renders granular instead of cascading.

## Quick Reference

| Prop carries… | Pass as | Why |
|---|---|---|
| A value the child reads | `ReadSignal<T>` | Auto-coerces from `Signal<T>`, `Memo<T>`, and plain `T`. Reactive contexts subscribe correctly. |
| A callback to the parent | `EventHandler<T>` | Keeps data flow one-way. No mutable props. |
| A value truly unchanged per render | `T` (plain) | No ceremony. But if it's derived from signals, hoist as `ReadSignal<T>`. |
| Writable shared state | **Don't.** Send a callback up | Mutable props break one-way flow and silently break reactivity downstream. |

## Why hoist

Large components grow until they're hard to read. The natural fix is
splitting them — but a naïve split turns local `use_signal` reads into
prop drilling that loses reactivity. Hoisting is "split the component,
leave the signal at the common ancestor, pass down as `ReadSignal<T>`".

## Pattern 1 — Split a component, keep state at the ancestor

Before — everything lives in one component:

```rust
#[component]
fn EmailAndName() -> Element {
    let mut name = use_signal(|| "name".to_string());
    let mut email = use_signal(|| "email".to_string());
    let is_valid = use_memo(move || validate_name_and_email(name, email));
    rsx! {
        if !is_valid() { "Invalid name or email" }
        input { oninput: move |e| name.set(e.value()) }
        input { oninput: move |e| email.set(e.value()) }
    }
}
```

After — `Validator` is its own component; `is_valid` is **hoisted** at
the parent, passed down as `ReadSignal<bool>`:

```rust
#[component]
fn EmailAndName() -> Element {
    let mut name = use_signal(|| "name".to_string());
    let mut email = use_signal(|| "email".to_string());
    let is_valid = use_memo(move || validate_name_and_email(name, email));
    rsx! {
        Validator { is_valid }
        input { oninput: move |e| name.set(e.value()) }
        input { oninput: move |e| email.set(e.value()) }
    }
}

#[component]
fn Validator(is_valid: ReadSignal<bool>) -> Element {
    rsx! { if !is_valid() { "Invalid name or email" } }
}
```

## Pattern 2 — Hoist mutation as a callback

Never accept a mutable signal prop to write back into the parent. Emit
an `EventHandler`; the parent decides how to mutate.

❌ **Anti-pattern** — breaks one-way flow:
```rust
#[component]
fn Incrementer(mut sig: Signal<i32>) -> Element {
    rsx! { button { onclick: move |_| sig += 1, "Increment" } }
}
```

✅ **Callback up, state stays put:**
```rust
#[component]
fn Parent() -> Element {
    let mut count = use_signal(|| 0);
    rsx! { Incrementer { onclick: move |_| count += 1 } }
}

#[component]
fn Incrementer(onclick: EventHandler<MouseEvent>) -> Element {
    rsx! { button { onclick: move |e| onclick.call(e), "Increment!" } }
}
```

## Pattern 3 — `use_memo` at the parent, `ReadSignal<T>` at the child

Rule: if more than one child (or any effect) needs the derived value,
memoize it once at the common ancestor. Each child gets a `ReadSignal`
and subscribes only to what it reads.

```rust
let is_valid = use_memo(move || validate(name, email));   // parent
Validator { is_valid }                                    // child subscribes
SubmitButton { enabled: is_valid }                        // sibling also subscribes
```

Both children now re-render only when `is_valid` actually flips — not on
every keystroke in `name` or `email`.

## Why `ReadSignal<T>` instead of plain `T`

From the docs: "Only reactive types like Signal, Memo, Resource, and
ReadSignal will participate in the Dioxus reactivity system… Rust
primitives are *not* reactive types."

Concretely: a `use_effect` that captures a plain `bool` prop **never
re-runs** when the source signal changes. Wrap the prop in
`ReadSignal<bool>` and the effect fires correctly.

❌ **Broken — effect doesn't fire on change:**
```rust
#[component]
fn Validator(is_valid: bool) -> Element {
    use_effect(move || log!("validity: {is_valid}"));   // never runs again
    rsx! { if !is_valid { "Invalid" } }
}
```

✅ **Tracked:**
```rust
#[component]
fn Validator(is_valid: ReadSignal<bool>) -> Element {
    use_effect(move || log!("validity: {is_valid}"));
    rsx! { if !is_valid() { "Invalid" } }
}
```

`ReadSignal<T>` auto-coerces from `Signal<T>`, `Memo<T>`, and primitives,
so the call site never changes: `Validator { is_valid }` works whether
`is_valid` is a memo, a signal, or a literal `true`.

## Common Mistakes

| Mistake | Fix |
|---|---|
| Accepting `bool` / `String` / primitive as a prop derived from state | Wrap in `ReadSignal<T>`. Effects and memos in the child will subscribe correctly. |
| Accepting `mut sig: Signal<T>` so the child can write | Replace with `on_change: EventHandler<T>`; keep state at the parent. |
| `let x = prop();` at the top of the component, then using `x` in RSX | Read the prop **inline** (`"{prop}"` or `prop()` inside `{}`) so each usage subscribes individually. |
| Recomputing a derivation in every child | Hoist to the nearest common ancestor as `use_memo`, pass down. |
| Duplicating the same signal across siblings ("lifted" by value) | Lift the signal, not the value. Each sibling gets the same `ReadSignal<T>`. |
| Passing a `Signal<T>` mutable just to avoid writing a callback | Accept the small cost of an `EventHandler<T>` — one-way data flow prevents a whole class of bugs. |

## Checklist

When a component is growing too large:

- [ ] Identify the shared pieces of state (signals + memos) needed by
      multiple future children.
- [ ] Leave those declarations at the ancestor. Don't duplicate.
- [ ] Extract children whose only reactive inputs can be expressed as
      `ReadSignal<T>` props.
- [ ] Replace any `.set`/`.write` a child would do with an
      `EventHandler<T>` prop invoked via `on_*.call(value)`.
- [ ] In each child, read props **inline** in RSX — not at the top of
      the function — so subscriptions stay narrow.
- [ ] Confirm: no `Signal<T>` / `Store<T>` / `&mut` in any child's
      prop list. Only `ReadSignal<T>`, `EventHandler<T>`, and plain `T`.

## Interactions with this project

The project uses a four-tier taxonomy (`global → organism → molecule → atom`).
Hoisting rules apply per tier:

- **Atoms** in `components/` accept only plain types or `ReadSignal<T>` +
  `EventHandler<T>`. No state, no reactive backbone imports.
- **Molecules** in `components/` may own behaviour-local state
  (see `components/share_chip.rs`'s `copied`, `components/nickname_input.rs`'s
  `draft`). They still receive reactive inputs as `ReadSignal<T>` and emit
  callbacks upward.
- **Organisms** and **globals** under `pages/{feature}/` are allowed to
  accept `Store<T>` / `Signal<T>` as props; that's by design for
  store-aware glue, not a hoisting violation. They derive props for
  molecules + atoms via `use_memo`.

See [rules/architecture.md](../../rules/architecture.md) for the full
tier rules, and [dioxus-state](../dioxus-state/SKILL.md) for Signal vs
Store decisions.
