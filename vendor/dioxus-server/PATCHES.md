# Local patches to `dioxus-server` 0.8.0-alpha.3

This directory is the published `dioxus-server` 0.8.0-alpha.3 crate with
[`upstream.diff`](upstream.diff) applied, wired in through `[patch.crates-io]`
in the workspace `Cargo.toml`. The diff makes the crate build **and run** on
`wasm32-unknown-unknown` inside the Cloudflare Worker (`tantrix-worker`), which
server-renders the app. Native builds keep upstream's dependency features and
behaviour.

Upstream has no wasm32 server support yet: DioxusLabs/dioxus#3275 is open. The
open DioxusLabs/dioxus#6041 would still leave a wasm32 server with `web`
unsupported. The fork stays until then; it is not meant for upstreaming.

`just vendor-check` downloads the release named in `Cargo.toml` and fails unless
this directory equals that release plus `upstream.diff`. It is not part of CI;
run it after touching this directory.

## Layout

- Everything except this file and `upstream.diff` comes from
  <https://static.crates.io/crates/dioxus-server/dioxus-server-0.8.0-alpha.3.crate>
  (sha256 `9bd04a249b1a6335ff245606fa600e93a1f6f795186116d444e6f17c57a8246c`).
- Its packaging files are left out: `Cargo.lock` (unused inside a workspace),
  `Cargo.toml.orig` (inherits from the Dioxus workspace) and
  `.cargo_vcs_info.json`. The manifest's generated header still mentions
  `Cargo.toml.orig`.

## Rules for the diff

- Add `#[cfg]` / `#[cfg_attr]` lines and wasm32-only items; leave upstream
  lines alone unless a line itself must change.
- Move a dependency to one platform by renaming its table header, so the
  version and features stay upstream's.
- Keep the sources rustfmt-clean (`rustfmt --edition 2024 src/lib.rs` formats
  the crate), as upstream is.
- Cargo shows a path dependency's warnings, unlike a registry crate's, so the
  diff also silences the ones our builds would print (hunks 6 and 13).
- After an edit, run `just vendor-diff`, update the hunk list below, then run
  `just vendor-check`.

## Hunks

In `upstream.diff` order.

### `Cargo.toml`

1. **axum `ws` and `tokio` move to a native-only `axum` table.** The `tokio`
   feature enables `tokio/net`, and `ws` requires it; tokio stops the build on
   wasm32 when `net` or `fs` is enabled. Native builds merge both tables, so
   axum keeps all of upstream's features. `http1` and `http2` build on wasm32
   and stay.
2. **`dioxus-devtools` is native-only.** Its `serve` feature calls a
   native-only `connect` and does not compile on wasm32. Only `launch.rs`,
   which is native-only already, uses the crate.
3. **`hyper-util` is native-only.** Its `full` feature pulls in `socket2` and
   `tokio/net`. Only `launch.rs` uses it. `hyper` itself builds on wasm32 and
   stays.
4. **`tower-http` is native-only.** Its `fs` feature enables `tokio/fs`. Only
   static file serving uses it (hunks 10, 12 and 13).
5. **wasm32 depends on `wasm-bindgen-futures`**, which `src/local_pool.rs`
   spawns on. wasm32 `tokio` stays optional and off, as upstream has it: with
   hunk 15, wasm32 code no longer names tokio.

### `src/document.rs`

6. **`reset_renderer` is native-only.** Only `launch` calls it, so Worker
   builds warned about dead code.

### `src/lib.rs`

7. **`mod local_pool` on wasm32, and `pub use launch::{router, serve}` gated
   like `mod launch`.** Upstream gates the module but not these re-exports, so
   the crate does not compile on wasm32.
8. **`IndexHtml` is public.** `ServeConfig::with_index_html(IndexHtml)` is
   public, but its argument type was `pub(crate)`. A Worker has no `public/`
   folder to read `index.html` from, so `tantrix-worker` parses the dx client's
   page, embedded at build time, with `IndexHtml::new`.

### `src/local_pool.rs` (new)

9. **A single-threaded `LocalPoolHandle`.** `FullstackState` renders pages and
   runs server functions on `tokio_util::task::LocalPoolHandle`, which spawns
   one OS thread per core; the Worker's first request panics with `failed to
   spawn thread`. On wasm32, `spawn_pinned` runs the task on the JS event loop.
   Like tokio's, its `JoinHandle` can be awaited or aborted (dropping the
   response stream stops the render), and dropping it detaches the task.

### `src/server.rs`

10. **Imports.** wasm32 takes `LocalPoolHandle` from `local_pool`. The static
    file serving imports (`ServiceExt`, `MapResponse`,
    `ServeFileSystemResponseBody`) are native-only, and `Path`, used only by
    that code, may go unused on wasm32.
11. **`DioxusRouterExt` is implemented on every target.** `tantrix-worker` calls
    `serve_api_application`. On wasm32 `serve_static_assets` returns the router
    unchanged: the Workers assets binding serves static files.
12. **Static file serving is native-only:** `serve_dir_cached`,
    `MappedAxumService`, `cache_response_forever` and
    `file_name_looks_immutable`. Its upstream `#[test]` stays ungated, so the
    crate's tests do not compile for wasm32; nothing builds them there.
13. **The `ServeDir` import may go unused in release builds.** Upstream uses it
    only under `debug_assertions`, so native release builds (`just run`,
    `just test-e2e`) warned.

### `src/ssr.rs`

14. **Imports.** wasm32 takes `LocalPoolHandle` and `JoinHandle` from
    `local_pool`; native imports tokio's `JoinHandle`.
15. **The render stream holds `Option<JoinHandle<()>>`** instead of
    `tokio::task::JoinHandle<()>`, so it accepts either handle.

## Upgrading to the next release

1. Check whether upstream now builds a server for wasm32 (DioxusLabs/dioxus#3275
   and the release notes). If it does, delete this directory and the
   `[patch.crates-io]` entry instead, and depend on the release.
2. Bump Dioxus in the workspace: the `dioxus*` dependencies,
   `tantrix-worker`'s exact `dioxus-server` pin and the dx CLI. Search the
   repository for the old version string to find the docs that name it.
3. Run `just vendor-update <version>`; it refuses to run over uncommitted
   changes in this directory. It downloads the release, replaces
   everything here except this file and `upstream.diff`, and applies the diff.
   A hunk that no longer applies is left in a `*.rej` file next to its source;
   a file the release no longer has is reported as missing, with no `.rej`.
   Redo those hunks by hand using the list above (`upstream.diff` still holds
   them), then delete the `.rej` files.
4. Recheck the reasons above against the new sources: new wasm32 gates
   upstream, new uses of tokio `net`/`fs` or threads outside `launch.rs`, and
   changes to how `LocalPoolHandle` or `JoinHandle` are used. Drop any hunk
   upstream made unnecessary; add the smallest gate for any new failure.
5. Verify with `just lint` (Clippy for the wasm32 Worker included), `just test`,
   `just worker-e2e` (Worker SSR) and `just test-e2e` (native fullstack build).
6. Run `just vendor-diff` and `just vendor-check`, update this file (version,
   checksum and hunk list), and commit.

## Not changed

`dioxus::server` (the umbrella crate's `server` feature) still does not compile
for wasm32, because it re-exports `serve`. Depend on `dioxus-server` directly,
as `tantrix-worker` does.
