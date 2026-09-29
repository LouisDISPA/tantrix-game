---
name: dioxus-routing
description: Dioxus routing with Route enum, nested routes, parameters, Link, and Navigator. Use when adding or modifying routes and navigation.
allowed-tools: Read, Grep, Glob, Edit, Write
---

# Dioxus Routing

## Quick Reference

| Task | Pattern |
|------|---------|
| Define routes | `#[derive(Routable, Clone, PartialEq)] enum Route { ... }` |
| Static route | `#[route("/")] Home {}` |
| Dynamic param | `#[route("/user/:id")] User { id: usize }` |
| Query param | `#[route("/search?:q")] Search { q: String }` |
| Catch-all (404) | `#[route("/:..path")] NotFound { path: Vec<String> }` |
| Layout wrapper | `#[layout(NavBar)] ... #[end_layout]` |
| URL nesting | `#[nest("/blog")] ... #[end_nest]` |
| Redirect | `#[redirect("/old", \|\| Route::Home {})]` |
| Link component | `Link { to: Route::Home {}, "Home" }` |
| Programmatic nav | `let nav = use_navigator(); nav.push(Route::X {})` |

## Route Enum

Routes are an enum with `#[derive(Routable)]`:

```rust
#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]  // preserve route structure formatting
enum Route {
    #[route("/")]
    Home {},

    #[route("/about")]
    About {},

    #[route("/user/:id")]
    UserProfile { id: usize },

    #[route("/:..path")]
    NotFound { path: Vec<String> },
}
```

Each variant must have a matching component function (same name, fields become props).

**Matching order:** top to bottom, first match wins. Place specific routes before dynamic ones, catch-all last.

## Nested Routes

### Layouts (`#[layout]`)

Wrap routes in shared UI (header, nav, sidebar):

```rust
#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(AppShell)]
        #[route("/")]
        Home {},
        #[route("/about")]
        About {},
    #[end_layout]

    #[route("/login")]
    Login {},  // no layout
}

#[component]
fn AppShell() -> Element {
    rsx! {
        nav { Link { to: Route::Home {}, "Home" } }
        Outlet::<Route> {}   // REQUIRED — child routes render here
        footer { "Footer" }
    }
}
```

### URL Nesting (`#[nest]`)

Group routes under a URL prefix:

```rust
#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[nest("/blog")]
    #[layout(BlogLayout)]
        #[route("/")]
        BlogList {},           // /blog/
        #[route("/:slug")]
        BlogPost { slug: String },  // /blog/my-post
    #[end_layout]
    #[end_nest]
}
```

### Multiple nesting levels

```rust
#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(RootLayout)]
        #[route("/")]
        Home {},

        #[nest("/admin")]
        #[layout(AdminLayout)]
            #[route("/")]
            Dashboard {},
            #[route("/users/:id")]
            UserDetail { id: usize },
        #[end_layout]
        #[end_nest]
    #[end_layout]
}
```

## Route Parameters

### Path parameters

Type must implement `FromStr` + `Display`:

```rust
#[route("/user/:id")]
User { id: usize },

#[route("/blog/:year/:month/:slug")]
BlogPost { year: u32, month: u32, slug: String },
```

### Query parameters

```rust
#[route("/search?:q")]
Search { q: String },

#[route("/products?:category&:sort&:page")]
Products { category: String, sort: String, page: usize },

// Optional query params
#[route("/search?:q&:page")]
Search { q: String, page: Option<usize> },
```

### Hash fragments

```rust
#[route("/docs#:section")]
Docs { section: String },
```

### Combined

```rust
#[route("/blog/:slug?:highlight#:section")]
BlogPost { slug: String, highlight: String, section: String },
// URL: /blog/my-post?highlight=code#comments
```

### Catch-all segments

```rust
#[route("/files/:..path")]
Files { path: Vec<String> },
// /files/a/b/c → path = ["a", "b", "c"]
```

## Navigation

### Link component (declarative)

```rust
Link { to: Route::Home {}, "Home" }

// With active class styling
Link {
    to: Route::Home {},
    class: "nav-link",
    active_class: "active",  // applied when route matches
    "Home"
}

// External link (full page load)
Link { to: "https://example.com", "External" }

// New tab
Link { to: Route::About {}, new_tab: true, "About" }
```

### Programmatic navigation

```rust
let nav = use_navigator();

// Push (adds to history — back button works)
nav.push(Route::Home {});
nav.push(Route::User { id: 42 });

// Replace (no history entry — good for redirects)
nav.replace(Route::Dashboard {});

// History
nav.go_back();
nav.go_forward();
nav.can_go_back();    // bool
nav.can_go_forward(); // bool
```

### Redirect pattern (auth guard)

```rust
#[component]
fn ProtectedPage() -> Element {
    let nav = use_navigator();
    let user = use_context::<Signal<Option<User>>>();

    use_effect(move || {
        if user.read().is_none() {
            nav.replace(Route::Login {});
        }
    });

    rsx! { /* protected content */ }
}
```

### Get current route

```rust
let route = use_route::<Route>();
match route {
    Route::Home {} => { /* ... */ },
    _ => {},
}
```

### Pre-built components

```rust
rsx! {
    GoBackButton { "← Back" }
    GoForwardButton { "Forward →" }
}
```

## Redirects (declarative)

```rust
#[redirect("/old-home", || Route::Home {})]
#[redirect("/blog-post/:id", |id: String| Route::BlogPost { id })]
```

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| Missing `Outlet::<Route> {}` in layout | Every layout MUST render `Outlet` or children won't show |
| Missing `#[end_layout]` / `#[end_nest]` | Always close layout/nest blocks |
| Catch-all route before specific routes | Put catch-all (`/:..path`) last in enum |
| Dynamic param type doesn't impl `FromStr` | Implement `FromStr` for custom types |
| `rustfmt` mangles route structure | Add `#[rustfmt::skip]` above enum |
| Using `push` for login redirect | Use `replace` to prevent back-button loop |
| Component name doesn't match variant | Variant `Home {}` needs `fn Home() -> Element` |

## Checklist

- [ ] Route enum derives `Routable, Clone, PartialEq`
- [ ] Each variant has a matching component function
- [ ] All layouts include `Outlet::<Route> {}`
- [ ] Layout/nest blocks have matching `#[end_layout]` / `#[end_nest]`
- [ ] Catch-all route is last in enum
- [ ] `#[rustfmt::skip]` on enum to preserve structure
- [ ] Active class set on navigation links
- [ ] Auth redirects use `replace`, not `push`
