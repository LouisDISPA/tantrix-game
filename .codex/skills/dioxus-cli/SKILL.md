---
name: dioxus-cli
description: Dioxus CLI (dx 0.8.0-alpha.3) commands, this project's Dioxus.toml, features, hot-reload, web/Worker builds and desktop/Android packaging with Velopack. Use when configuring builds, running the dev server, packaging, or debugging CLI issues.
allowed-tools: Read, Grep, Glob, Bash
---

# Dioxus CLI & Configuration

The app crate is `tantrix-app`; its binary is `tantrix`. CI pins
`DIOXUS_CLI_VERSION: '0.8.0-alpha.3'` (`.github/workflows/release.yml`); keep
the local `dx` on the same version as the `dioxus` dependency. Run recipes from
the workspace root; `dx` itself runs from `tantrix-app/` (or with `--package tantrix-app`).

## Quick Reference — Project Commands

| Command | What it runs |
|---------|-------------|
| `just dev` | `dx serve --web --fullstack true --features fullstack,devtools --hot-reload true` (port 8080) |
| `just run` | `dx run --web --fullstack true --features fullstack --release` |
| `just desktop` | `dx serve --desktop --features desktop,bundle` |
| `just desktop-bundle-macos` | `dx bundle --release --macos --features desktop,bundle --package-types macos` |
| `just fmt` | `dx fmt` (RSX formatting) |
| `just fmt-check` | `cargo fmt --all --check` (what CI checks) |
| `just lint` / `just test` | Clippy (workspace, `server`, wasm32 `web`) / workspace + server tests |
| `just worker-e2e` / `just worker-deploy` | Client build + Worker SSR build, then browser tests or `wrangler deploy` |

Useful raw `dx` commands: `dx check`, `dx doctor`, `dx build --keep-names`
(for `twiggy top` size work), `dx tools assets` (used by `scripts/build-worker-ssr.sh`).

## Features (`tantrix-app/Cargo.toml`)

| Feature | Use |
|---------|-----|
| `web` | Browser client (`dioxus/web`, timers, fetch) |
| `server` | Axum + SQLite backend, room sockets |
| `fullstack` | `dioxus/fullstack`; combine with `web`/`server` through `dx --fullstack` |
| `devtools` | Hot reload + Dioxus logger for `dx serve`; never in release builds |
| `desktop` | Native desktop (WebView), native sockets, Velopack updates |
| `bundle` | Release/desktop packaging switches |
| `worker` | SSR inside the Cloudflare Worker (`tantrix-worker`) |
| `android` | Android build: `mobile` WebView UI + native sockets and installer updates |
| `mobile`, `native`, `liveview` | Renderers (`android` pulls in `mobile`) |

## Configuration (`tantrix-app/Dioxus.toml`)

```toml
[application]
name = "Tantrix"
android_min_sdk_version = 34
android_main_activity = "android/MainActivity.kt"

[android]                  # min 34, target/compile 36
[web]
pre_compress = true

[web.watcher]
reload_html = true
watch_path = ["src", "assets", "public"]

[bundle]
identifier = "io.github.louisdispa.tantrix"
icon = ["icons/…png", "icons/icon.icns", "icons/icon.ico"]

[bundle.macos]
bundle_name = "Tantrix"

[bundle.deb]
depends = ["libwebkit2gtk-4.1-0", "libgtk-3-0"]
```

There is no `out_dir`: release web output lands in
`target/dx/tantrix/release/web/public` (named after the binary, not
`target/dx/tantrix-app/...`). Desktop bundles go to `--out-dir bundles`.

## Production Web Client

`worker-client-build` (justfile) builds the browser client with `@client` /
`@server` argument groups: the client rebuilds std (`-Zbuild-std`) with
`-Cpanic=immediate-abort` and `-Zlocation-detail=none`; the server half uses the
stock toolchain. Panics abort silently in the browser.

- Keep dx symbols: hydration needs them. Don't strip symbols or debug info.
- Never add `-Zfmt-debug=none` — `Color`'s `Display`, keys and `data-*`
  attributes go through `Debug`.
- Measure Wasm with `twiggy top` on a `dx build --keep-names` copy.

## Hot-Reload

`just dev` serves with `--hot-reload true` and the `devtools` feature. RSX and
CSS changes patch in place; Rust logic changes rebuild. `public/` is watched, so
manifest/icon edits reload too.

## Packaging

Dioxus produces the payloads; Velopack (`vpk` 1.2.161) produces the downloads,
feeds and update packages. Dioxus does not cross-build native desktop bundles:
build on the target OS.

| Target | dx step | Packager | Downloads |
|--------|---------|----------|-----------|
| macOS | `dx bundle --platform macos --package-types macos --features desktop,bundle --out-dir bundles` | `scripts/pack-desktop.py macos <arch> <version>` | DMG |
| Linux | `dx bundle --platform linux --package-types deb --features desktop,bundle --out-dir bundles` | `pack-desktop.py` (+ `scripts/bundle-appimage.sh`) | AppImage, `.deb` |
| Windows | `dx build --platform windows --features desktop,bundle` | `pack-desktop.py` (+ `pack-windows.ps1`) | Setup EXE, portable ZIP |
| Android | `dx build --platform android --features android` (inside the script) | `scripts/pack-android.py` | Signed ARM64 APK |

Packagers write to `artifacts/`. Release builds pass `--locked`. Tags
`tantrix-app-v<version>` publish through `.github/workflows/release.yml`. See
[desktop releases](../../../doc/desktop-release.md),
[updates and signing](../../../doc/desktop-updates.md) and
[Android releases](../../../doc/android-release.md).

## Testing

```bash
just test                                  # workspace + server tests
cargo test -p tantrix-app                  # app only
cargo test -p tantrix-app --features server
just test-e2e                              # Playwright, native backend
```

App tests mount components through `util::test_page::GamePage`; no `dx`
needed. Prove changes with tests rather than launching the app.

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| `cargo run` for the web app | `just dev` / `just run` (dx handles Wasm and fullstack) |
| Looking for output in `target/dx/tantrix-app/` | It's `target/dx/tantrix/…` (binary name) |
| `dx` version differs from `dioxus` | Install the pinned `0.8.0-alpha.3` CLI |
| `--package-types dmg` / `msi` | Bundle `macos` / `deb` (or `dx build` on Windows), then `pack-desktop.py` |
| Cross-building a desktop bundle | Build on the target OS (CI matrix) |
| `devtools` in a release build | Only `just dev` enables it |
| Stripping symbols to shrink Wasm | Breaks hydration; find size elsewhere |

## Checklist

- [ ] Local `dx --version` matches `DIOXUS_CLI_VERSION`
- [ ] Features match the target (`fullstack`, `desktop,bundle`, …)
- [ ] `just fmt-check`, `just lint`, `just test` pass
- [ ] Release/package commands use `--locked`
- [ ] No `-Zfmt-debug=none`, no symbol stripping
