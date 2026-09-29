# Browser end-to-end tests

The tests start the Dioxus fullstack app on port 4173, including its local
signaling server. They run in headless Chromium.

The full-game tests play through a local match and a two-browser online match.
They check turn progression, board synchronization, the empty bag, and final
scores. The online test uses the real WebRTC connection between browsers.

Install the repository's nightly Rust toolchain, the `wasm32-unknown-unknown`
target, Dioxus CLI 0.7.4, Node.js, and the browser once:

```bash
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli --version 0.7.4 --locked
cd e2e
npm ci
npx playwright install chromium
```

From the repository root, run `just e2e`. To run a single file or inspect a
failure, use `cd e2e && npx playwright test tests/local-game.spec.ts` or
`npx playwright show-report`. Playwright starts and stops `dx run` itself;
port 4173 must be free.
