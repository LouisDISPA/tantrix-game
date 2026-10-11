# Browser end-to-end tests

Eight Chromium tests cover the important user flows. CI uses the native Axum
server with an in-memory SQLite database. The same suite can be run manually
against the Cloudflare Worker.

| Spec | Coverage |
| --- | --- |
| `local-game` | Start, place a tile, advance the turn, end and play again; one Expert AI turn. |
| `forced-after-free-move` | Four seeded placements show that a forced fill keeps the turn. |
| `tile-drag` | A desktop drag commits on release under the production script policy; an invalid drop places nothing. |
| `mobile-play` | Phone taps select a tile, position and rotation; End Game asks for confirmation; real touch drags, pans and pinches. |
| `online` | Two independent sessions join, play synchronized moves, reload and resume, and see matching results; a missing room shows an error. |

Complete-game playthroughs, repeated join attempts, detailed geometry and
animation assertions, and room-rule permutations are excluded. Game rules and
scoring live in `tantrix-game` tests; room validation, seats, disconnects,
replacement, second-tab takeover, origin checks and cookies are covered in
`tantrix-room`. Drag and gesture state have focused tests in `tantrix-app`.

## Setup

Follow the [repository setup](../README.md), then install Chromium:

```bash
just setup-e2e
cd e2e && pnpm exec playwright install chromium
```

## Native runs

Build and run the suite from the repository root:

```bash
just native-e2e
```

Playwright's native config runs `dx run --release`, which builds and starts the
app on port 4175, then waits for `/health`. Cargo reuses unchanged compilation
outputs. CI runs format and lint checks, a production build, unit tests and e2e
tests in one job. The production build and Playwright use the same release
options and target directory, so e2e reuses the compiled client and server.
The Rust cache retains dependency and workspace compilation outputs between
runs. Playwright keeps its build command and starts a fresh process in CI;
locally, it reuses an already running server on that port. Each page waits for
client hydration before interacting.

For focused runs:

```bash
cd e2e
pnpm run test:native tests/online.spec.ts
pnpm run test:native --workers=2
```

CI uses four workers and one retry; local runs have no retries. The plain
`pnpm exec playwright test` command also selects the native config.

To test an already running app at another address, omit server startup with:

```bash
BASE_URL=http://127.0.0.1:8080 pnpm run test:native
```

The server has ten minutes to build and start; the entire Playwright run has a
fifteen-minute limit so failures still produce a report before CI's job timeout.

## Worker runs

```bash
just worker-e2e
```

This builds the production client and Worker before testing. To repeat tests
without rebuilding, run `pnpm run test:worker` from `e2e/` after the build. Wrangler
uses port 4174 and can reuse a running local instance.

## Manual screenshots

Two optional cases capture a seeded board at phone and desktop sizes. They are
skipped during normal runs and write to the git-ignored `e2e/screenshots/`:

```bash
cd e2e
TANTRIX_SCREENSHOTS=1 pnpm run test:native tests/screens.spec.ts
```

## Seeded games and failures

The four-move fixture at `fixtures/forced-after-free-move.json` is shared with
Rust tests. Regenerate it after a deliberate change to dealing with:

```bash
cargo test --workspace print_forced_after_free_move_fixture -- --ignored --nocapture
```

The online flow attaches a merged console/request/WebSocket timeline on failure.
It excludes private tokens and snapshot contents. CI records a Playwright trace
on the first retry and uploads failure reports. For local diagnosis, use:

```bash
cd e2e
pnpm run test:native tests/online.spec.ts --trace on
```
