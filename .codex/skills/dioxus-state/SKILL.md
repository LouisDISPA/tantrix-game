---
name: dioxus-state
description: Dioxus state management — signals, hooks, context, global state, and decisions about what to wrap as a signal, Signal<T> vs Store<T>, where state lives, and how components should consume it. Use when designing or refactoring any reactive state.
allowed-tools: Read, Grep, Glob, Edit, Write
---

# Dioxus State Management

## Quick Reference

| Need | Use | Scope |
|------|-----|-------|
| Local mutable state | `use_signal(|| value)` | Single component |
| Derived/computed value | `use_memo(move || expr)` | Single component |
| Side effect on change | `use_effect(move || { ... })` | Single component |
| Share state in subtree | `use_context_provider` + `use_context` | Component tree |
| App-wide state | `static X: GlobalSignal<T> = Signal::global(|| v)` | Entire app |
| Async state channel | `use_coroutine(|rx| async { ... })` | Single component |
| Run async task (no return) | `use_future(|| async { ... })` | Single component |
| Async data fetching | `use_resource(|| async { ... })` | Single component |

## Deciding what to wrap as a signal

Most state-shape bugs come from wrapping values in signals by reflex.
Answer these questions in order before reaching for `use_signal`.

### Does it actually need to be reactive?

A value needs to be a signal only if **changes must trigger re-render**.
If it never changes — or if changes don't need to propagate — leave it as
a plain `T`.

```rust
// NO — constants never change
let hex_size = use_signal(|| 50.0);
// YES
const HEX_SIZE: f64 = 50.0;

// NO — loop-iteration value isn't reactive
fn BoardTile(m: ReadSignal<Move>) -> Element { ... }
// YES — just a value the parent has in hand
fn BoardTile(m: Move) -> Element { ... }
```

**Test:** if every caller passes a plain value (loop variable, constant,
once-per-render compute), the prop should be plain, not `ReadSignal<T>`.

### Is it derived from other signals?

If yes, use `use_memo` — not a separate `Signal` you hand-sync.

```rust
// NO — manual sync, certain to drift
let mut doubled = use_signal(|| 0);
use_effect(move || doubled.set(count() * 2));

// YES — auto-tracked
let doubled = use_memo(move || count() * 2);
```

## `Signal<T>` vs `Store<T>`

`#[derive(Store)]` gives **field-level reactivity**: each field becomes
its own signal, so subscribing to `foo.x()` doesn't re-render on `foo.y`
writes.

Use `Store<T>` when **both**:
- Fields change independently (most mutations touch 1–2 fields).
- Some readers care about a narrow slice and shouldn't re-render for
  unrelated field changes.

Use plain `Signal<T>` when the struct is an **atomic unit** — every
mutation touches many fields at once, or every reader needs the whole
value. Field-level subscriptions buy nothing, and `.field()()` access
just adds noise.

### Worked example (this codebase)

`UiState` has ~10 fields that change independently: phase, hovered,
zoom, pan_x, selected_tile, new_tile, highlighted_color, … A pan drag
writes 3 camera fields. StatusBar reads only `phase`. PlayerHand reads
only `selected_tile`/`new_tile`. Field-level subscription saves ~8
re-renders per drag frame.

```rust
#[derive(Debug, Clone, PartialEq, Store)]
pub struct UiState { pub phase: TurnPhase, pub zoom: f64, /* … */ }
```

`GameState` holds `board`, `bag`, `hands`, `turn_tracker`, `hand_info`.
Every `play_move` updates nearly all of them. There's no "subscribe to
just the bag" use case — changes always arrive together.

```rust
#[derive(Debug, Clone)]    // ← no Store derive
pub struct GameState { ... }

// In context:
pub game_state: Signal<GameState>,
pub ui_state:   Store<UiState>,

// Reads naturally:
let gs = ctx.game_state.read();    // plain struct field access
gs.board.len();
ctx.ui_state.phase()();            // field-level subscription
```

## Where the signal lives

| Scope | Primitive | When |
|---|---|---|
| Single component | `use_signal` | Truly local state — form input draft, hover state on one element |
| Subtree (a page, a panel) | `use_context_provider` + `use_context` | Shared across 2+ levels of components |
| App-wide | `GlobalSignal` | Theme, logged-in user — things that outlive any page |

Avoid prop-drilling signals through 3+ levels. Avoid `GlobalSignal` for
page-scoped state — they leak across mount/unmount and complicate tests.

## Consuming signals in components

### Read narrowly, at the deepest scope that cares

```rust
// NO — reads every UiState field; any write re-renders this component
let ui = ui_state.read();
let phase = ui.phase;

// YES — subscribes only to phase
let phase = ui_state.phase()();
```

### Lift derived state **into** children, not out to parents

If a piece of derived state is only used by one child, let that child
compute it. Don't calculate it in the parent's render loop and drill it
as a prop.

```rust
// NO — parent computes per-child state
for m in placed_moves() {
    let colors = highlight_map.get(&m.position).cloned().unwrap_or_default();
    let just_placed = last_placed() == Some(m.position);
    rsx! { BoardTile { m, highlight_colors: colors, just_placed } }
}

// YES — child memoizes its own state from context
for m in placed_moves() {
    rsx! { BoardTile { m } }
}

#[component]
fn BoardTile(m: Move) -> Element {
    let highlights = use_memo(move || {
        use_highlight_map().read().get(&m.position).cloned().unwrap_or_default()
    });
    // …
}
```

Two wins: smaller parent, and only the affected child re-renders when
`highlight_map` changes — not the whole loop.

**Don't lift when** the parent already uses the value itself, or when the
value is cheaper to take from a loop variable than to re-read from context
in each of 50+ children.

### Prop types: plain `T` is the default

| Pass as | When |
|---|---|
| `T` | 99% of cases — child is rendered with a value the parent has right now |
| `ReadSignal<T>` | Caller owns a signal and wants the child to re-render independently (rare — `use_context` is usually cleaner) |
| `Signal<T>` | Child needs to *write* the caller's signal (rarest — prefer `EventHandler<T>` or context) |

If every caller does `use_signal(|| ...)` just to pass a `ReadSignal<T>`,
the prop should just be `T`.

## Signals (`use_signal`)

Create reactive state that automatically tracks reads and triggers re-renders:

```rust
let mut count = use_signal(|| 0);

// Reading
let val: i32 = count();          // clone value
let guard = count.read();        // borrow (holds read lock)
let val = count.peek();          // read WITHOUT subscribing

// Writing
count.set(42);                   // replace
*count.write() += 1;             // mutate via guard
count += 1;                      // operator overload (AddAssign, SubAssign, etc.)
count.with_mut(|v| *v += 1);     // mutate via closure

// Display in RSX
rsx! { p { "Count: {count}" } }  // auto-subscribes
```

**Vec/Option helpers on signals:**
```rust
let mut items = use_signal(|| vec![1, 2, 3]);
items.push(4);                   // direct Vec methods
items.pop();
items.len();

let mut opt = use_signal(|| Some(42));
opt.take();                      // Option methods
opt.replace(Some(100));
```

**Borrow rules** (runtime, like RefCell):
```rust
// WRONG — overlapping borrows panic
let read = signal.read();
signal += 1;  // panics!

// RIGHT — scope the read guard
{ let read = signal.read(); /* use it */ }
signal += 1;

// RIGHT — use with() for scoped access
signal.with(|v| println!("{v}"));
signal += 1;

// WRONG — holding guard across .await
let val = signal.read();
some_future.await;  // panics!

// RIGHT — clone before async
let val = signal();
some_future.await;
```

## Derived State (`use_memo`)

Automatically recompute when dependencies change:

```rust
let mut count = use_signal(|| 0);
let doubled = use_memo(move || count() * 2);
let quadrupled = use_memo(move || doubled() * 2);  // chain memos

rsx! { p { "4x = {quadrupled}" } }
```

Memo vs Signal: memos are **read-only** and **auto-update** when their signal dependencies change.

## Side Effects (`use_effect`)

Run code when reactive dependencies change:

```rust
let mut count = use_signal(|| 0);

use_effect(move || {
    println!("Count changed to: {}", count());
    // runs on mount and whenever count changes
});
```

## Context (`use_context_provider` + `use_context`)

Share state across a component subtree without prop drilling:

```rust
// Parent — provide context (once, at top of component)
#[derive(Clone, Copy)]
struct AppContext {
    theme: Signal<String>,
    user: Signal<Option<User>>,
}

fn App() -> Element {
    let ctx = use_context_provider(|| AppContext {
        theme: Signal::new("light".into()),
        user: Signal::new(None),
    });
    rsx! { Child {} }
}

// Any descendant — consume context
fn Child() -> Element {
    let ctx: AppContext = use_context();
    rsx! { p { "Theme: {ctx.theme}" } }
}

// Safe alternative (returns Option)
let maybe_ctx: Option<AppContext> = try_use_context();
```

**Key points:**
- Provide before any child that reads it
- Wrap mutable data in `Signal` for reactivity
- Context propagates down the tree only
- Fast lookup (hash map based)
- Use for deeply nested state (>2 levels); prefer props for 1-2 levels

## Global State

App-wide state accessible from anywhere, no provider needed:

```rust
static COUNT: GlobalSignal<i32> = Signal::global(|| 0);
static DOUBLED: GlobalMemo<i32> = Signal::global_memo(|| COUNT() * 2);

fn AnyComponent() -> Element {
    rsx! {
        button { onclick: move |_| *COUNT.write() += 1, "+1" }
        p { "Count: {COUNT}, Doubled: {DOUBLED}" }
    }
}
```

**Limitations:**
- Constructor must be a function pointer (no closures)
- Persists for entire app lifetime
- Can complicate testing (state leaks between tests)

**When to use global vs context:**

| Global | Context |
|--------|---------|
| Truly app-wide (theme, auth) | Subtree-scoped |
| Single instance | Multiple instances possible |
| Simple access pattern | Better testability |

## Coroutines (`use_coroutine`)

Long-running async task with a message channel:

```rust
use futures::StreamExt;

let coroutine = use_coroutine(|mut rx: UnboundedReceiver<String>| async move {
    while let Some(msg) = rx.next().await {
        println!("Got: {msg}");
    }
});

// Send messages from handlers
rsx! {
    button { onclick: move |_| coroutine.send("hello".into()), "Send" }
}
```

## Hook Rules

1. **Top level only** — don't call hooks inside if/match/loops
2. **Same order every render** — Dioxus tracks hooks by call order
3. **Components/hooks only** — not from regular functions

```rust
// WRONG
fn MyComponent() -> Element {
    if some_condition {
        let sig = use_signal(|| 0);  // conditional hook!
    }
}

// RIGHT
fn MyComponent() -> Element {
    let sig = use_signal(|| 0);  // always called
    if some_condition { /* use sig */ }
}
```

## Performance

- **Read signals narrowly** — read inside the smallest scope needed; only the subscribing node re-renders, not the entire component
- **Use `use_memo` for expensive computations** — recalculates only when dependencies actually change
- **Split components around state** — isolate frequently-updating signals into small child components so siblings don't re-render
- **Batch mutations** — `signal.with_mut(|v| { v.push(a); v.push(b); })` triggers one update instead of two
- **Avoid large clones** — use signals and borrowing; don't clone large structs on every render
- **Debounce rapid input** — delay processing for search/filter signals to reduce re-render frequency
- **`peek()` for logging** — read without subscribing: `signal.peek()` won't cause re-renders
- **Keep components shallow** — Dioxus renders scopes top-to-bottom by height; frequently-updated state belongs in shallow components
- **Profile first** — use Chrome DevTools Performance tab before optimizing

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| Holding `read()` across `.await` | Clone with `signal()` before async |
| Overlapping `read()` and `write()` | Scope read guard or use `with()` |
| Conditional hook call | Always call hooks at top level |
| Forgetting `move` on closures | `use_effect(move \|\| { ... })` |
| Using `use_signal` for derived values | Use `use_memo` instead |
| Global signal with closure | Use fn pointer: `Signal::global(\|\| val)` |
| Context not found panic | Ensure provider is in ancestor, or use `try_use_context` |
| Modifying signal during render | Use `use_effect` for side effects |

## Checklist

- [ ] Local state uses `use_signal`
- [ ] Derived values use `use_memo` (not manually computed)
- [ ] Side effects are in `use_effect` (not inline in render)
- [ ] Shared state uses context (2+ levels) or global (app-wide)
- [ ] Context provider is above all consumers in the tree
- [ ] No hooks inside conditionals or loops
- [ ] Signal guards are scoped and not held across await points
- [ ] Global signals use function pointers, not closures
