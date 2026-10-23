# Browser tests

Chromium tests run against native Axum (CI, in-memory SQLite) or the Worker.

| Spec | Coverage |
| --- | --- |
| `local-game` | Start, play, advance, end, replay; one Expert AI turn |
| `forced-after-free-move` | Four seeded placements: forced fill keeps the turn |
| `tile-drag` | Commit on release under production script policy; reject invalid drop |
| `mobile-play` | Tap selection, End Game confirmation, touch drag, pan, pinch |
| `online` | Two sessions synchronize, reload/resume and finish; rejected-action feedback; missing-room error; server-assigned code and permanent game URL |
| `online-recovery` | Socket interruption, snapshot synchronization, bounded retries, seat loss, results, second-tab takeover, silent transport and background suspension; phone/desktop and both themes |
| `startup` | Direct visits to home, online and download hydrate, dismiss the loader and enable content without browser errors |

Optional `screens` cases capture phone/desktop boards. Core rules and scoring are tested in `tantrix-game`; room rules, sessions and persistence in `tantrix-room` and backend tests; gesture state in `tantrix-app`.

## Setup and runs

Follow [development setup](../doc/development.md), then install Chromium:

```sh
just setup-e2e
cd e2e && pnpm exec playwright install chromium
```

From the repository root:

```sh
just test-e2e      # native: build/start release app at port 4175
just worker-e2e    # build client + Worker, start Wrangler at port 4174
```

Focused/repeated runs from `e2e/`:

```sh
pnpm run test:native tests/online.spec.ts
pnpm run test:native --workers=2
pnpm run test:worker    # requires a prior Worker build
BASE_URL=http://127.0.0.1:8080 pnpm run test:native
```

`DIOXUS_CLI=/absolute/path/to/dx` selects a versioned CLI for native startup. `BASE_URL` skips server startup for either backend. Otherwise, local runs reuse an existing server; CI starts a fresh process. Plain `pnpm exec playwright test` selects native. Every page waits for hydration.

Native startup has ten minutes; Worker startup has one minute; the suite has fifteen minutes. Local runs use eight workers and no retries. CI native runs use four workers and one retry; Worker runs use eight workers. CI's production build shares release outputs with native startup, and Rust caches retain compilation outputs. The Worker job uses `just worker-e2e` to rebuild both browser and SSR assets before starting Wrangler.

## Screenshots and diagnosis

Optional screenshots, including the recovery notices from `online-recovery`, go to git-ignored `e2e/screenshots/`:

```sh
cd e2e
TANTRIX_SCREENSHOTS=1 pnpm run test:native tests/screens.spec.ts
pnpm run test:native tests/online.spec.ts --trace on
```

Online failures attach a merged console/request/WebSocket timeline without tokens or snapshot contents. CI captures traces on first retry and uploads failure reports.

The fixture `fixtures/forced-after-free-move.json` is shared with Rust tests. After deliberately changing dealing, regenerate it from the repository root:

```sh
cargo test --workspace print_forced_after_free_move_fixture -- --ignored --nocapture
```
