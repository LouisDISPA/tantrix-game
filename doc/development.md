# Development

## Setup

- Install [rustup](https://rust-lang.github.io/rustup/). `rust-toolchain.toml` selects nightly, Clippy, rustfmt, rust-src and the Wasm target.
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
| `just test-e2e` | Native-server browser suite |
| `just worker-e2e` | Build client + Worker and run the same browser suite |
| `just worker-deploy` | Build and deploy the Worker |

CI runs format, lint, production build, unit tests and native browser tests in one job; browser startup reuses the production compilation outputs. A separate job runs `just worker-e2e`, rebuilding matching browser and Worker SSR assets before running the browser suite against Wrangler. Keep the CLI and framework on the same exact prerelease version.

See [browser tests](../e2e/README.md), [Worker operations](../cloudflare/README.md) and [desktop releases](desktop-release.md).
