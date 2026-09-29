---
name: dioxus-spawn
description: Running futures in Dioxus with spawn, spawn_forever, async event handlers, and use_action. Use when launching background tasks from event handlers, or when a task is being mysteriously cancelled before it starts.
allowed-tools: Read, Grep, Glob, Edit, Write
---

# Running Futures in Dioxus

This skill is about launching futures (one-shot background work) from event
handlers and lifecycle hooks. For reactive data-fetching see `dioxus-async`
(`use_resource`, `SuspenseBoundary`).

Reference: [Dioxus 0.7 — Running Futures](https://dioxuslabs.com/learn/0.7/essentials/basics/async#running-futures-with-use-action).

## Quick reference

| Need | Use |
|---|---|
| Async event handler | `onclick: move \|_\| async move { ... }` (auto-spawned on the event's scope) |
| Manual task tied to the current component | `spawn(async move { ... }) -> Task` |
| Task that must outlive the caller's component | `spawn_forever(async move { ... }) -> Task` |
| UI-triggered async with pending / cancel-prior-call semantics | `use_action(my_server_fn)` |
| Cancel a spawned task | `task.cancel()` |

## The scope-lifetime trap (the important one)

`spawn` attaches the future to the **current component scope** — the scope
that was active when `spawn` was called. When that scope is destroyed
(component unmounts), Dioxus cancels the task, **even if it never polled**.

This is invisible most of the time because tasks usually get their first
poll before the parent re-renders. It bites in this scenario:

1. User clicks a component (say, a tile on a board).
2. The `onclick` synchronously mutates state such that this very component
   will no longer be rendered next frame.
3. Before returning, `onclick` calls `spawn(async move { ... })` — which
   attaches the task to the component's scope.
4. Dioxus processes the state change and re-renders. The clicked component
   unmounts, taking its scope — and the task — with it.
5. The spawned future is cancelled before its first poll. No error, no
   warning. Just silence.

**Rule of thumb:** if the future's lifetime should outlive the component
that launched it, use `spawn_forever`. If you need to cancel when the
launcher unmounts (typical for polling / subscriptions), `spawn` is right.

### Concrete example from this codebase (Tantrix AI loop)

`state/actions.rs::advance_turn` only flips the phase to `WaitingForAi`.
`pages/local_game/mod.rs::LocalGame` owns a `use_effect` that reacts to
that phase and calls `spawn(async move { run_ai_loop(ctx).await; ... })`.
That keeps the task in the page scope: it survives transient clicked
tiles/hexes unmounting, but is cancelled when the local game page unmounts.

### `spawn_forever` has a gotcha of its own

⚠️ Signals captured by a `spawn_forever` future must remain valid for the
**lifetime of the app**. If the captured signal lives in a component that
unmounts, accessing it from the still-running future will panic.

Safe: capture signals owned by the root or a long-lived context (e.g. the
app-level `GameCtx` — it's `Copy` and the stores live as long as the
`LocalGame` component it was provided in).

Unsafe: capture a signal from a child component that can unmount.

## The four ways to run a future

### 1. Async event handler — the default

```rust
rsx! {
    button {
        onclick: move |_| async move {
            let data = fetch_data().await;
            state.set(data);
        },
        "Load"
    }
}
```

Dioxus auto-spawns the returned future on the button's scope. Same
lifetime semantics as `spawn` — cancelled if the button unmounts.

Use this whenever you're not doing anything fancy with the returned `Task`.

### 2. `spawn(fut)` — manual, scoped

```rust
use dioxus::prelude::*;

let task = spawn(async move {
    gloo_timers::future::TimeoutFuture::new(1000).await;
    tracing::info!("one second later");
});
// later, if desired:
task.cancel();
```

Returns a `Task` handle. Cancelled on component unmount.

### 3. `spawn_forever(fut)` — manual, root-scoped

```rust
use dioxus::core::spawn_forever;

spawn_forever(async move {
    run_ai_loop(ctx).await;
    settle_turn(ctx);
});
```

Runs on `ScopeId::ROOT`. Lifetime = app lifetime. Use when:

- The future outlives the component that triggered it (AI loops,
  long-running work, turn handoffs, background syncs).
- The caller is a transient UI element (menu item, dialog button,
  momentary hex on a board) that will unmount during the work.

### 4. `use_action(fn)` — for UI-triggered mutations

Tailored for user-submitted async actions with built-in pending and
auto-cancellation of the previous call:

```rust
let mut action = use_action(create_user);

rsx! {
    button {
        onclick: move |_| action.call(new_user.clone()),
        "Create"
    }
    if action.pending() { "Saving..." }
    if let Some(Err(e)) = action.result() { "Error: {e}" }
}
```

Each `.call()` cancels any in-flight call and starts fresh. Good for
debounced form submissions.

## Decision tree

```
Is it async work triggered by user interaction?
├── Returns a value you display?             → use_resource (see dioxus-async)
├── Returns Result<T> to a button/form?      → use_action
├── Short-lived, dies with the button OK?    → async-move event handler OR spawn
└── Must outlive the clicked component?      → spawn_forever
```

## Common mistakes

| Mistake | Fix |
|---|---|
| `spawn` from a handler on a component that will unmount this frame | `spawn_forever` — attaches to root scope |
| Task silently disappears, no error, no first log line | Scope was killed mid-tick. Switch to `spawn_forever`. |
| `spawn_forever` captures a signal from a component that can unmount | Capture from a root-scope signal or a `GlobalSignal` / `Copy` context |
| Holding `signal.read()` guard across `.await` | Clone scalar value before suspending: `let v = signal();` |
| Using `spawn` for long background loops (polling, subscriptions) | Fine if they *should* die on unmount; `spawn_forever` if they shouldn't |
| Forgetting `async move` in event handlers | `onclick: move \|_\| async move { ... }` — both `move`s needed |
| Not handling `play_move` / action errors in spawned loops | Check `Result` and `break`; the default `let _ = ...` hides hangs |

## Diagnosing "my task never runs"

Symptom: you call `spawn(...)`, it returns a `Task`, but the future's
first line (say `info!("started")`) never fires.

Add logs at these points to localize the cause:

```rust
info!("about to spawn");
let task = spawn(async move {
    info!("task: entered async block");   // ← if this is missing, scope died
    // ...
});
info!("spawn returned, task={task:?}");
```

- If `task: entered async block` fires → the task started; problem is
  somewhere after (panic, deadlock, or guard held across await).
- If it never fires → the task was cancelled before its first poll.
  **99% of the time this is the scope-lifetime trap** — swap `spawn` for
  `spawn_forever`.

## Checklist

- [ ] Tasks that should survive the triggering component use `spawn_forever`
- [ ] Tasks that should die with the component use `spawn` (or an async
      event handler)
- [ ] Signals captured by `spawn_forever` futures live as long as the app
      (root-owned / context / `GlobalSignal`)
- [ ] No signal `.read()` guards held across `.await` points
- [ ] Async event handlers use `move |_| async move { ... }` (both `move`s)
- [ ] User-triggered mutations with pending state use `use_action`
- [ ] Spawned loops handle errors explicitly — no silent `let _ = ...` on
      calls that can fail in ways that would cause the loop to hang
