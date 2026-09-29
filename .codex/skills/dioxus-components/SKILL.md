---
name: dioxus-components
description: Dioxus component patterns, RSX syntax, props, conditional rendering, lists, and event handlers. Use when creating or modifying Dioxus components.
allowed-tools: Read, Grep, Glob, Edit, Write
---

# Dioxus Components & RSX

## Quick Reference

| Task | Pattern |
|------|---------|
| Simple component | `fn Name() -> Element { rsx! { ... } }` |
| Component with props | `#[component] fn Name(prop: Type) -> Element` |
| Optional prop | `prop: Option<Type>` or `#[props(default)] prop: Type` |
| Children | `children: Element` parameter |
| Event callback | `on_click: EventHandler<()>` |
| Conditional render | `if cond { rsx! { ... } }` inside `rsx!` |
| List render | `for item in iter { rsx! { div { key: "{id}", ... } } }` |
| Dynamic class | `class: if active { "on" }` (multiple `class:` attrs merge) |

## RSX Syntax

RSX uses brace-based syntax (not angle brackets like JSX):

```rust
rsx! {
    div {
        class: "container",
        h1 { "Title" }
        p { "Text with {variable} interpolation" }
        button {
            onclick: move |_| count += 1,
            "Click me"
        }
    }
}
```

**Key rules:**
- HTML elements are lowercase: `div`, `span`, `button`
- Components are PascalCase: `MyComponent {}`
- Attributes use colons: `class: "foo"`, `id: "bar"`
- Text is quoted strings: `"Hello {name}!"`
- Rust expressions in braces: `{some_value}`
- Reserved words need `r#` prefix: `r#type: "text"`

**Conditional attributes** (unterminated `if`):
```rust
rsx! {
    div {
        class: "base",
        class: if is_active { "active" },
        class: if has_error { "error" },
    }
}
```

**Control flow** directly in RSX:
```rust
rsx! {
    if show { p { "Visible" } }

    match status {
        Status::Loading => rsx! { "Loading..." },
        Status::Ready => rsx! { "Done" },
    }

    for (id, item) in items.iter().enumerate() {
        div { key: "{id}", "{item}" }
    }
}
```

## Components

Function components return `Element` (alias for `Result<VNode, RenderError>`):

```rust
fn MyComponent() -> Element {
    rsx! { div { "Hello" } }
}
```

With the `#[component]` macro (recommended — auto-generates Props struct):

```rust
#[component]
fn Greeting(name: String) -> Element {
    rsx! { div { "Hello, {name}!" } }
}
```

Components are **memoized by default** — they only re-render when props change (via `PartialEq`) or subscribed signals update.

## Props

### Using `#[component]` (recommended)

```rust
#[component]
fn UserCard(
    name: String,
    age: i32,
    #[props(default = "user.png".to_string())] avatar: String,
    #[props(default)] disabled: bool,       // defaults to false
    size: Option<u32>,                       // optional, defaults to None
    #[props(into)] label: String,            // accepts &str via Into
) -> Element {
    let size = size.unwrap_or(64);
    rsx! { div { "{name}, {age}" } }
}
```

### Manual Props struct

```rust
#[derive(Props, PartialEq, Clone)]
struct ButtonProps {
    text: String,
    color: String,
}

fn Button(props: ButtonProps) -> Element {
    rsx! { button { "{props.text}" } }
}
```

### Children

```rust
#[component]
fn Card(title: String, children: Element) -> Element {
    rsx! {
        div { class: "card",
            h2 { "{title}" }
            {children}
        }
    }
}

// Usage
rsx! { Card { title: "Hi", p { "Content" } } }
```

### Event handler props

```rust
#[component]
fn ClickableCard(title: String, on_click: EventHandler<()>) -> Element {
    rsx! {
        div {
            onclick: move |_| on_click.call(()),
            "{title}"
        }
    }
}

// Usage
rsx! {
    ClickableCard {
        title: "Click me",
        on_click: move |_| count += 1,
    }
}
```

### Extending HTML attributes

```rust
#[component]
fn CustomDiv(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! { div { ..attributes, {children} } }
}

// Usage — pass any HTML attribute
rsx! { CustomDiv { class: "foo", id: "bar", "Content" } }
```

### ReadSignal props (reactive)

```rust
#[component]
fn Display(count: ReadSignal<i32>) -> Element {
    let doubled = use_memo(move || count() * 2);
    rsx! { p { "Count: {count}, Doubled: {doubled}" } }
}

let count = use_signal(|| 0);
rsx! { Display { count } }
```

## Event Handlers

| Event | Type | Common use |
|-------|------|-----------|
| `onclick` | `MouseEvent` | Buttons, clickable elements |
| `oninput` | `FormEvent` | Text input (fires on every keystroke) |
| `onchange` | `FormEvent` | Input committed (blur/enter) |
| `onsubmit` | `FormEvent` | Form submission |
| `onkeydown` | `KeyboardEvent` | Keyboard shortcuts |
| `onmouseenter/leave` | `MouseEvent` | Hover effects |
| `onwheel` | `WheelEvent` | Scroll/zoom |
| `onfocus/onblur` | `FocusEvent` | Focus management |

```rust
rsx! {
    input {
        value: "{text}",
        oninput: move |e| text.set(e.value()),
    }
    form {
        onsubmit: move |e| {
            e.prevent_default();
            // handle submit
        },
    }
    div {
        tabindex: 0,  // required for keyboard events on non-input elements
        onkeydown: move |e| {
            if e.key() == Key::Escape { /* ... */ }
        },
    }
}
```

**Key methods on events:**
- `e.value()` — input value (FormEvent)
- `e.key()` — key pressed (KeyboardEvent)
- `e.coordinates()` — mouse position (MouseEvent)
- `e.modifiers()` — shift/ctrl/alt state
- `e.prevent_default()` — prevent browser default
- `e.stop_propagation()` — stop event bubbling

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| `<div>` JSX syntax | Use `div { }` brace syntax |
| `className: "x"` | Use `class: "x"` |
| `type: "text"` | Use `r#type: "text"` (reserved word) |
| Missing `move` on closures | Always use `move \|_\|` for event handlers |
| Props not Clone + PartialEq | Derive both on manual Props structs |
| Forgetting `key` in loops | Add `key: "{unique_id}"` to loop items |
| `onclick: count += 1` | Use `onclick: move \|_\| count += 1` (closure) |
| Passing Signal as prop | Use `ReadSignal<T>` type for reactive props |

## Checklist

- [ ] Component name is PascalCase
- [ ] Returns `Element`
- [ ] Props implement Clone + PartialEq (auto with `#[component]`)
- [ ] Event handlers use `EventHandler<T>` for callbacks
- [ ] Lists have unique `key` attributes
- [ ] Reserved HTML attributes use `r#` prefix
- [ ] Children use `children: Element` parameter
- [ ] No JSX-style angle bracket syntax
