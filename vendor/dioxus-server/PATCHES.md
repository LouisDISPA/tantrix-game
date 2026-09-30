# Local patches to `dioxus-server` 0.7.10

This is the published `dioxus-server` 0.7.10 crate, vendored and wired in via
`[patch.crates-io]` in the workspace `Cargo.toml`. Everything is upstream except
the items below, all of which make the crate build **and run** on
`wasm32-unknown-unknown` inside a Cloudflare Worker (`tantrix-worker`). Native
builds keep upstream's exact dependency features and behaviour.

Drop this directory (and the `[patch]` entry) once upstream supports wasm32
servers — see DioxusLabs/dioxus#3275. Upstream `main` (0.8.0-alpha.1) has the
same problems, at the same lines.

## 1. Networking dependencies are native-only (`Cargo.toml`)

Upstream enables axum `ws`/`tokio`/`http1`/`http2`, `hyper-util/full`,
`tokio-tungstenite` (default `connect`) and `tower-http/fs` for every target.
They all pull in tokio `net`/`fs`, which do not compile for wasm32 (`mio`).
They are only used by `launch`/`serve` (already `cfg(not(wasm32))`) and by
static file serving, so they moved to `[target.'cfg(not(target_arch =
"wasm32"))'.dependencies]` with the same features. `dioxus-devtools/serve`
moved the same way.

The wasm32 `tokio` dependency was `optional = true` with no feature enabling
it, so wasm32 builds had no tokio at all; it is now a plain dependency.

## 2. Single-threaded render pool (`src/local_pool.rs`)

`FullstackState` renders every page (and runs every server function) on a
`tokio_util::task::LocalPoolHandle`, which eagerly spawns one OS thread per
core. In a Worker the first request panics with `failed to spawn thread:
operation not supported on this platform`.

On wasm32, `ssr.rs` and `server.rs` import a drop-in `LocalPoolHandle` whose
`spawn_pinned` runs the future on the JS event loop
(`wasm_bindgen_futures::spawn_local`), with an abortable `JoinHandle` so
dropping the response stream still cancels the render.

## 3. `DioxusRouterExt` on wasm32 (`src/server.rs`)

The trait impl was `cfg(not(wasm32))` although `serve_api_application` is
documented as being for WebAssembly hosts. It is now available on every
target; on wasm32 `serve_static_assets` is a no-op (the host serves static
files) and the `tower-http` file-serving helpers are compiled out.

`launch::{router, serve}` re-exports are gated like the rest of `launch`
(`src/lib.rs`), since they did not compile for wasm32, and so is
`document::reset_renderer`, which only `launch` calls.

## 4. `IndexHtml` is public (`src/lib.rs`)

`ServeConfig::with_index_html(IndexHtml)` is public but `IndexHtml` was
`pub(crate)`, so it could not be called. Hosts without a `public/` directory
next to the executable (Workers have no filesystem) need it to provide dx's
client `index.html`.

## Not changed

`dioxus::server` (the umbrella crate's `server` feature) still does not
compile for wasm32, because it re-exports `serve`. Depend on `dioxus-server`
directly, as `tantrix-worker` does.
