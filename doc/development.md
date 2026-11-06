# Development

## Setup

- Install [rustup](https://rust-lang.github.io/rustup/). `rust-toolchain.toml` pins a dated nightly, Clippy, rustfmt, rust-src and the Wasm target; CI and release builds use the same toolchain. Nightly is only needed for the browser client's `-Zbuild-std`. Bump the date deliberately.
- On Linux, the desktop checks in `just lint` and `just test` need `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev`.
- Install `just` and Dioxus CLI 0.8.0-alpha.3, e.g. `cargo binstall just dioxus-cli@0.8.0-alpha.3`.
- Use Node.js 24 (`.node-version`) and pnpm 12.8.1 (pinned in package manifests).
- For `just tunnel`, install [cloudflared](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/downloads/).

```sh
just setup-e2e
just setup-worker
cd e2e && pnpm exec playwright install chromium
```

## Commands

Run `just` for the complete list.

| Command | Purpose |
| --- | --- |
| `just dev` | Fullstack web app with hot reload, port 8080 |
| `just run` | Production web build and server, port 8080 |
| `just tunnel` | Share the local server through a temporary tunnel |
| `just desktop` | Desktop development app |
| `just fmt-check` / `just lint` / `just test` | CI Rust checks |
| `just ci` | Every CI check, then the native browser suite |
| `just test-e2e` | Native-server browser suite |
| `just worker-e2e` | Build client + Worker and run the same browser suite |
| `just worker-deploy` | Build and deploy the Worker |

CI runs on pushes to `main` and on pull requests, in two jobs. One runs `just fmt-check`, `just lint` and `just test`: Clippy and tests for the default, server and desktop features, plus Clippy for the wasm32 web client and for the Worker against a stand-in client page. The other builds the release app and runs the native browser suite, whose startup reuses that build. Only `main` saves Rust caches. CI does not build the Worker, the rebuilt-std browser client or desktop bundles: run `just worker-e2e` before deploying the Worker. A release tag runs CI on the tagged commit before building bundles. Keep the CLI and framework on the same exact prerelease version.

See [browser tests](../e2e/README.md), [Worker operations](../cloudflare/README.md), [desktop releases](desktop-release.md) and [Android releases](android-release.md).
