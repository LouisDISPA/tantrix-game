---
name: dioxus-async
description: Dioxus async patterns, use_resource, suspense, server functions, and fullstack features. Use when working with async data, server functions, or SSR.
allowed-tools: Read, Grep, Glob, Edit, Write
---

# Dioxus Async & Fullstack

## Quick Reference

| Need | Use |
|------|-----|
| Fetch async data (returns value) | `use_resource(|| async { ... })` |
| Run async task (no return) | `use_future(|| async { ... })` |
| Loading placeholder UI | `SuspenseBoundary { fallback: ..., Child {} }` |
| Error fallback UI | `ErrorBoundary { handle_error: ..., Child {} }` |
| Server-side RPC | `#[server] async fn name() -> Result<T>` |
| Call server fn from UI | `use_resource(|| async { my_server_fn().await })` |
| Server fn with action | `let action = use_action(my_server_fn);` |

## `use_resource`

Manages async data fetching with automatic dependency tracking:

```rust
let mut user_id = use_signal(|| 1);

let user = use_resource(move || async move {
    let id = user_id();  // auto-tracks this signal
    reqwest::get(format!("/api/users/{id}"))
        .await?
        .json::<User>()
        .await
});
```

**Reading resource values:**
```rust
// Pattern match on Option<Result<T, E>>
match &*user.read_unchecked() {
    Some(Ok(data)) => rsx! { "Got: {data}" },
    Some(Err(err)) => rsx! { "Error: {err}" },
    None => rsx! { "Loading..." },
}

// Or use .suspend() with SuspenseBoundary
let data = user.suspend()?;  // returns value or throws suspension
rsx! { "Data: {data}" }
```

**Control methods:**
```rust
resource.restart();   // cancel and re-fetch
resource.cancel();    // stop fetching
resource.pause();     // pause
resource.resume();    // resume
resource.clear();     // clear value
resource.pending();   // bool — is loading?
resource.finished();  // bool — is done?
```

**Reactive dependencies** — resource auto-restarts when any signal read inside the closure changes:
```rust
let mut search = use_signal(|| String::new());
let results = use_resource(move || async move {
    let q = search();  // re-runs when search changes
    fetch_results(&q).await
});
```

## Suspense

Declarative loading states with `SuspenseBoundary`:

```rust
rsx! {
    SuspenseBoundary {
        fallback: |context| rsx! {
            div { "Loading {context.suspended_futures().len()} items..." }
        },
        AsyncComponent {}
    }
}

#[component]
fn AsyncComponent() -> Element {
    let data = use_resource(|| async { fetch_data().await });
    let value = data.suspend()?;  // suspends until ready
    rsx! { "Data: {value}" }
}
```

**Nested boundaries** for independent loading:
```rust
rsx! {
    SuspenseBoundary {
        fallback: |_| rsx! { "Loading user..." },
        UserInfo {}
    }
    SuspenseBoundary {
        fallback: |_| rsx! { "Loading posts..." },
        PostsList {}
    }
}
```

**Combined with ErrorBoundary:**
```rust
rsx! {
    ErrorBoundary {
        handle_error: |err| rsx! { "Error: {err}" },
        SuspenseBoundary {
            fallback: |_| rsx! { "Loading..." },
            AsyncContent {}
        }
    }
}
```

## Server Functions

Call server-side code from the client with `#[server]`:

```rust
#[server]
async fn get_user(id: u32) -> Result<User, ServerFnError> {
    // This runs on the server only
    let user = db.find_user(id).await?;
    Ok(user)
}

// Named endpoints (recommended for desktop/mobile)
#[get("/api/users/{id}")]
async fn get_user(id: u32) -> Result<User> { ... }

#[post("/api/users")]
async fn create_user(user: CreateUser) -> Result<User> { ... }
```

**Calling from components:**
```rust
// Via use_resource
let user = use_resource(move || async move {
    get_user(42).await
});

// Via use_action (for mutations)
let mut action = use_action(create_user);
rsx! {
    button {
        onclick: move |_| action.call(new_user.clone()),
        "Create"
    }
    if action.pending() { "Saving..." }
}
```

**Server extractors** (headers, cookies, state):
```rust
#[post("/api/posts", headers: HeaderMap, state: State<AppState>)]
async fn create_post(post: CreatePost) -> Result<Post> {
    let auth = headers.get("authorization");
    let db = &state.db;
    // ...
}
```

**Error handling patterns:**
```rust
// Simple (anyhow-style)
#[server]
async fn get_data() -> Result<String> {
    Ok(reqwest::get("...").await?.text().await?)
}

// Status code errors
#[get("/api/user/{id}")]
async fn get_user(id: u32) -> Result<User, StatusCode> {
    find_user(id).ok_or(StatusCode::NOT_FOUND)
}

// HTTP error helpers
#[get("/api/user/{id}")]
async fn get_user(id: u32) -> Result<User> {
    find_user(id).or_not_found("User not found")?
    // also: bad_request(), unauthorized(), forbidden(), internal_server_error()
}
```

**Streaming responses:**
```rust
#[get("/api/logs")]
async fn stream_logs() -> Result<Streaming<String, JsonEncoding>> {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    tokio::spawn(async move { tx.unbounded_send("log".into()).ok(); });
    Ok(Streaming::new(rx))
}
```

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| Using `use_future` when you need the value | Use `use_resource` (returns data) |
| Not handling None/Loading state | Always match `None \| Some(Ok) \| Some(Err)` |
| Missing `SuspenseBoundary` with `.suspend()` | Wrap component tree in `SuspenseBoundary` |
| `.suspend()` without `?` | Must use `data.suspend()?` (returns Result) |
| Anonymous `#[server]` in desktop app | Use named endpoints (`#[get("/api/...")]`) |
| Holding signal read across await | Clone value before async: `let v = sig();` |
| No `ErrorBoundary` around async content | Always pair `ErrorBoundary` with `SuspenseBoundary` |
| Server fn not returning Result | Server functions must return `Result<T>` |

## Checklist

- [ ] Async data uses `use_resource` (not `use_future`)
- [ ] All three resource states handled (None, Ok, Err)
- [ ] `SuspenseBoundary` wraps components using `.suspend()`
- [ ] `ErrorBoundary` wraps async content for error display
- [ ] Server functions return `Result<T>`
- [ ] Server functions validate input (they're public HTTP endpoints)
- [ ] Signal values cloned before async operations
- [ ] Named endpoints used for desktop/mobile server functions
