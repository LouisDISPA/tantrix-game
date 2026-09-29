---
name: dioxus-styling
description: Dioxus styling patterns, CSS, asset management, and performance. Use when styling components or optimizing performance.
allowed-tools: Read, Grep, Glob, Edit, Write
---

# Dioxus Styling

## Quick Reference

| Approach | When to use |
|----------|-------------|
| Inline style attribute | One-off dynamic styles |
| Individual CSS props | Simple per-element styles |
| CSS classes + external file | Reusable, maintainable styles |
| CSS modules | Scoped styles, avoid naming conflicts |
| Tailwind CSS | Utility-first rapid prototyping |
| CSS custom properties | Theme system, design tokens |

## Inline Styles

### Style attribute (string)

```rust
rsx! {
    div {
        style: "color: red; font-size: 20px;",
        "Styled text"
    }
}
```

### Individual CSS properties (snake_case)

```rust
rsx! {
    div {
        color: "blue",
        font_size: "24px",
        background_color: "#f0f0f0",
        padding: "20px",
        border_radius: "8px",
        "Content"
    }
}
```

### Dynamic styles

```rust
let is_active = use_signal(|| true);
let count = use_signal(|| 0);

rsx! {
    div {
        color: if is_active() { "green" } else { "gray" },
        font_size: "{count() + 12}px",
        padding: "{count() * 2}px",
    }
}
```

## CSS Classes

### External stylesheets with `asset!`

```rust
const STYLE: Asset = asset!("/assets/style.css");

fn app() -> Element {
    rsx! {
        Stylesheet { href: STYLE }
        div { class: "container", "Content" }
    }
}
```

### Multiple/conditional classes

Multiple `class:` attributes are automatically merged:

```rust
rsx! {
    div {
        class: "button",
        class: if is_active() { "active" },
        class: if has_error() { "error" },
        "Button"
    }
}
```

## CSS Modules (scoped)

Avoid naming conflicts with scoped class names:

```rust
#[css_module("/assets/component.css")]
struct Styles;

rsx! {
    div { class: Styles::container,
        h1 { class: Styles::title, "Scoped Styles" }
    }
}
```

Use `:global` in CSS to opt out of scoping for specific classes.

## Tailwind CSS

Configure `tailwind.config.js` to scan `.rs` files, then:

```rust
const TAILWIND: Asset = asset!("/assets/tailwind.css");

fn app() -> Element {
    rsx! {
        Stylesheet { href: TAILWIND }
        div { class: "container mx-auto px-4",
            h1 { class: "text-4xl font-bold text-blue-600", "Title" }
        }
    }
}
```

Conditional Tailwind:
```rust
class: if is_active() { "bg-blue-500 text-white" } else { "bg-gray-200 text-gray-700" }
```

## Theme System (CSS Custom Properties)

This project uses CSS custom properties with OKLCH colors:

```css
/* main.css */
:root {
    --color-red: oklch(0.65 0.25 25);
    --color-blue: oklch(0.55 0.2 260);
    --bg-primary: #ffffff;
    --text-primary: #1a1a1a;
}

[data-theme="dark"] {
    --bg-primary: #1a1a1a;
    --text-primary: #f0f0f0;
}
```

Toggle theme via global signal:
```rust
static THEME: GlobalSignal<Theme> = Signal::global(|| Theme::Light);

fn toggle_theme() {
    *THEME.write() = match THEME() {
        Theme::Light => Theme::Dark,
        Theme::Dark => Theme::Light,
    };
}
```

## Asset Management

The `asset!()` macro handles CSS, images, and other static files:

```rust
// CSS
const STYLE: Asset = asset!("/assets/main.css");
rsx! { Stylesheet { href: STYLE } }

// Images
const LOGO: Asset = asset!("/assets/logo.png");
rsx! { img { src: LOGO } }
```

Assets are:
- Compile-time validated (missing file = build error)
- Automatically hashed for cache busting
- Optimized during production builds

## Performance

### Rendering
- **Key dynamic lists** — `key: "{id}"` enables efficient reordering; without keys, Dioxus diffs the entire list
- **Split large components** — smaller components re-render independently; keep frequently updated ones shallow in the tree
- **Granular signal reads** — read signals in the narrowest scope; only the subscribing text node updates, not the whole component
- **`use_memo` for derived values** — avoids recomputation; only recalculates when dependencies change

### State
- **Avoid large clones** — use signals and borrowing instead of cloning large structs on every render
- **Batch mutations** — `items.with_mut(|v| { v.push(a); v.push(b); })` instead of multiple individual pushes
- **Debounce rapid input** — add a delay before processing search/filter inputs to avoid excessive re-renders

### Async
- **Use `use_resource`** for batched requests instead of spawning many concurrent tasks
- **Wrap async in `SuspenseBoundary`** — prevents blocking the entire UI during data loads
- **Cancel on unmount** — drop pending requests when components unmount

### CSS & Assets
- Prefer CSS classes over inline styles (better browser caching)
- Use CSS custom properties for theme values (single repaint on theme change)
- Leverage `asset!()` for efficient loading and cache busting

### Build & Profiling
- **`dx build --release`** for production — enables LLVM optimizations and dead code elimination
- **Chrome DevTools Performance tab** — record and analyze component rendering
- **Profile before optimizing** — measure, don't guess

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| `className` (React habit) | Use `class` in Dioxus |
| `background-color` (kebab-case) | Use `background_color` (snake_case) for individual props |
| Missing `Stylesheet` component | Add `Stylesheet { href: asset!("...") }` in app root |
| CSS not updating in dev | Check hot-reload watches `/assets` directory |
| Style string without semicolons | `style: "color: red; font-size: 14px;"` (semicolons required) |

## Checklist

- [ ] Stylesheets loaded via `asset!()` macro
- [ ] `Stylesheet` component in app root for each CSS file
- [ ] Theme uses CSS custom properties (not inline styles)
- [ ] Dynamic styles use signals for reactivity
- [ ] CSS class names are lowercase-kebab-case
- [ ] No duplicate/conflicting class names (use CSS modules if needed)
