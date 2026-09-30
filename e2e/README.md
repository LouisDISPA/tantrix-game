# Browser end-to-end tests

Playwright runs headless Chromium against either the native Axum server or
`wrangler dev`. Both use the same Rust room rules and client protocol.

| Spec | Coverage |
| --- | --- |
| `local-game`, `full-game` (local) | Local play through the results screen. |
| `forced-after-free-move` | A seeded game (`/game?seed=N`) replays `fixtures/forced-after-free-move.json`: a free move that creates a forced space keeps the turn until it is filled. |
| `online`, `full-game` (online) | Two browsers create and join a room, play, and agree on results. |
| `online-flow` | Both boards, turns, bag count, and connection status after every move; reload and resume a seat. |
| `online-join` | Repeated room join handshake (`JOIN_ATTEMPTS`, default 5). |
| `online-lobby` | A rejected color claim leaves the player's old seat intact. |
| `online-retry` | A nonexistent code gives a clear error rather than creating a room. |
| `online-room-lifecycle` | The creator can leave, another player can start, and a late visitor can take over a seat. |
| `online-authority` | Invalid and stale move intents cannot mutate server game state. |
| `online-early-end` | A seated player ends a running game; both players see results, including after reload. |
| `board-layout` | Board controls stay clear of legal positions at several widths. |

## Setup

Install the repository's nightly Rust toolchain, the `wasm32-unknown-unknown`
target, Dioxus CLI, Node.js, and Chromium. Then install the e2e dependencies:

```bash
rustup target add wasm32-unknown-unknown
cd e2e && npm ci && npx playwright install chromium
```

## Run

From the repository root, `just native-e2e` builds and tests the native server;
`just worker-e2e` builds and tests the Worker. `just e2e` aliases the Worker
suite. For a focused Worker run after `just worker-build`:

```bash
cd e2e
npx playwright test tests/online-flow.spec.ts
JOIN_ATTEMPTS=50 npx playwright test tests/online-join.spec.ts
```

The Worker config starts Wrangler on port 4174; the native config starts
`dx run` on port 4175 with an in-memory SQLite database. Those ports must be free.
A cold Rust build can take several minutes.

## Seeded games

`/game?seed=N` deals the same bag and turn order every time, so a spec can
script exact moves: hand tiles, board hexes and rotation choices carry
`data-tile`, `data-position` (`x,y`) and `data-rotation` attributes. The Rust
tests in `tantrix-app` and `tantrix-room` replay the same fixture, so a change
that alters the deal fails `cargo test` first. Regenerate the fixture with:

```bash
cargo test --workspace print_forced_after_free_move_fixture -- --ignored --nocapture
```

## Failures

Online tests attach a merged timeline of both browsers to the Playwright
report and write it under `test-results/**/merged-timeline.log`. It records
page and console errors, failed requests, and room WebSocket message types.
Private session tokens and game snapshots are excluded from the diagnostic log.
