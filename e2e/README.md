# Browser end-to-end tests

Playwright runs headless Chromium against `wrangler dev`, with the production
Worker entry point, Rust game logic, and a room Durable Object. Build the web
client and Worker modules before running tests.

| Spec | Coverage |
| --- | --- |
| `local-game`, `full-game` (local) | Local play through the results screen. |
| `online`, `full-game` (online) | Two browsers create and join a room, play, and agree on results. |
| `online-flow` | Both boards, turns, bag count, and connection status after every move; reload and resume a seat. |
| `online-join` | Repeated room join handshake (`JOIN_ATTEMPTS`, default 5). |
| `online-lobby` | A rejected color claim leaves the player's old seat intact. |
| `online-retry` | A nonexistent code gives a clear error rather than creating a room. |
| `online-room-lifecycle` | The creator can leave, another player can start, and late joins are rejected. |
| `online-authority` | Invalid and stale move intents cannot mutate server game state. |
| `board-layout` | Board controls stay clear of legal positions at several widths. |

## Setup

Install the repository's nightly Rust toolchain, the `wasm32-unknown-unknown`
target, Dioxus CLI, Node.js, and Chromium. Then install the e2e dependencies:

```bash
rustup target add wasm32-unknown-unknown
cd e2e && npm ci && npx playwright install chromium
```

## Run

From the repository root, `just worker-e2e` builds the app and runs all specs.
`just e2e` is an alias. For a focused run after `just worker-build`:

```bash
cd e2e
npx playwright test tests/online-flow.spec.ts
JOIN_ATTEMPTS=50 npx playwright test tests/online-join.spec.ts
```

The Playwright config starts and stops Wrangler on port 4174. Port 4174 must
be free. A cold Rust build can take several minutes.

## Failures

Online tests attach a merged timeline of both browsers to the Playwright
report and write it under `test-results/**/merged-timeline.log`. It records
page and console errors, failed requests, and room WebSocket message types.
Private session tokens and game snapshots are excluded from the diagnostic log.
