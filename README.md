# Tantrix

Rust Tantrix game with a Dioxus app, native room server, and Cloudflare Worker.

## Setup

- Install [rustup](https://rust-lang.github.io/rustup/). The nightly toolchain,
  Clippy, rustfmt, and Wasm target are declared in `rust-toolchain.toml`.
- Install `just` and the prebuilt Dioxus CLI `dx` 0.7.10. With
  [cargo-binstall](https://github.com/cargo-bins/cargo-binstall):

  ```bash
  cargo binstall just
  cargo binstall dioxus-cli@0.7.10
  ```

- Use Node.js 24 (`.node-version`) and [pnpm](https://pnpm.io/installation)
  12.8.1. The package manifests pin the pnpm version.
- For `just tunnel`, install [cloudflared](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/downloads/).

```bash
just setup-e2e
just setup-worker
cd e2e && pnpm exec playwright install chromium
```

Run `just` to list commands. `just dev` starts the native app; `just tunnel`
shares it through a temporary Cloudflare Tunnel. `just native-e2e` and
`just worker-e2e` run the browser suites. See [e2e/README.md](e2e/README.md)
and [cloudflare/README.md](cloudflare/README.md) for details.
