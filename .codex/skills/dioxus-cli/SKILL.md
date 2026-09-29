---
name: dioxus-cli
description: Dioxus CLI commands, configuration (Dioxus.toml), hot-reload, bundling, and testing. Use when configuring builds, running dev server, or debugging CLI issues.
allowed-tools: Read, Grep, Glob, Bash
---

# Dioxus CLI & Configuration

## Quick Reference — CLI Commands

| Command | Description |
|---------|-------------|
| `dx new <path>` | Create new Dioxus project from template |
| `dx init` | Initialize Dioxus in existing project |
| `dx serve` | Dev server with hot-reload and file watching |
| `dx serve --hot-reload` | Explicit hot-reload (usually default) |
| `dx serve --open` | Open browser automatically |
| `dx serve --port 3000` | Custom port (default: 8080) |
| `dx serve --platform web` | Target specific platform |
| `dx build` | Production build |
| `dx build --release` | Optimized release build |
| `dx bundle` | Create platform-specific package |
| `dx bundle --package-types dmg` | macOS DMG bundle |
| `dx check` | Check for errors without building |
| `dx fmt` | Format RSX macros |
| `dx fmt --check` | Check RSX formatting (CI) |
| `dx translate` | Convert HTML to RSX syntax |
| `dx doctor` | Diagnose system setup |
| `dx self-update` | Update CLI to latest version |
| `dx run` | Build and run without dev features (CI/CD) |

### `dx serve` Interactive Shortcuts

| Key | Action |
|-----|--------|
| `r` | Force rebuild |
| `p` | Toggle auto-rebuild on file changes |
| `v` | Toggle verbose output |
| `/` | Show all available commands |
| `Ctrl+C` | Exit |

## Configuration (`Dioxus.toml`)

### Application section

```toml
[application]
name = "Tantrix"          # App name (required)
out_dir = "dist"           # Build output directory
asset_dir = "assets"       # Static assets directory
```

### Web configuration

```toml
[web.app]
title = "Tantrix"          # HTML <title>

[web.watcher]
watch_path = ["src", "assets"]   # Directories to watch for changes

[web.wasm_opt]
level = "z"                # WASM optimization: z (size), s (size), 0 (none), 1-4 (speed)
```

### Dev proxy (avoid CORS in development)

```toml
[[web.proxy]]
backend = "http://localhost:3001/api"
```

### Bundle configuration

```toml
[bundle]
identifier = "com.example.tantrix"
publisher = "Your Name"
icon = ["icons/icon.png"]
version = "0.1.0"
```

### Platform features

In `Cargo.toml`, Dioxus uses feature flags for platform targeting:

```toml
[features]
default = []
web = ["dioxus/web"]
desktop = ["dioxus/desktop"]
mobile = ["dioxus/mobile"]
native = ["dioxus/native"]
```

Build for specific platform: `dx build --features web`

## Hot-Reload

Hot-reload updates the UI without full rebuilds:
- **RSX changes** — instant update (no recompile)
- **Rust logic changes** — triggers recompile + reload
- **CSS changes** — instant if watched

Ensure `Dioxus.toml` watches the right paths:
```toml
[web.watcher]
watch_path = ["src", "assets"]
```

Enable with `dx serve --hot-reload` (usually on by default).

**Hot-patching** (`--hot-patch`) — experimental, patches Rust logic without full rebuild.

## Bundling

Create distributable packages:

```bash
# macOS
dx bundle --platform macos --package-types dmg

# Windows
dx bundle --platform windows --package-types msi

# Linux
dx bundle --platform linux --package-types appimage
dx bundle --platform linux --package-types deb

# iOS
dx bundle --platform ios --package-types ios-bundle

# Android
dx bundle --platform android --package-types apk
```

## Testing

### Unit tests (core logic)
```bash
cargo test -p tantrix-game
```

### Component tests
Dioxus provides a testing framework for component rendering:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::prelude::*;

    #[test]
    fn test_component_renders() {
        let mut dom = VirtualDom::new(MyComponent);
        dom.rebuild_in_place();
        // Assert on the rendered output
    }
}
```

### RSX formatting check (CI)
```bash
dx fmt --check
```

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| `cargo run` for web app | Use `dx serve` (handles WASM compilation) |
| Missing `Dioxus.toml` | Run `dx config init` or create manually |
| Wrong `watch_path` | Ensure `src` and `assets` are included |
| Port already in use | `dx serve --port 3001` (different port) |
| WASM build fails | Run `dx doctor` to check toolchain setup |
| `dx serve` not finding assets | Check `asset_dir` in `Dioxus.toml` matches actual path |
| Slow dev builds | Use `dx serve` (incremental), not `dx build` |
| Release build too large | Set `wasm_opt.level = "z"` for size optimization |

## Checklist

- [ ] `Dioxus.toml` exists with correct `name` and `asset_dir`
- [ ] `watch_path` includes all source and asset directories
- [ ] Platform features configured in `Cargo.toml`
- [ ] `dx doctor` passes (toolchain setup correct)
- [ ] `dx fmt --check` passes (RSX formatting)
- [ ] Production build uses `--release` flag
- [ ] Bundle config has correct `identifier` for distribution
