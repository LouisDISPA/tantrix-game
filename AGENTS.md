# Tantrix Game

Rust nightly workspace: nine crates for a 2–4-player hex-tile game. Dioxus 0.8.0-alpha.3 web/desktop UI; Ratatui 0.30.2 terminal UI. Players build their color's longest line or loop.

## Workspace map

| Crate | Responsibility |
| --- | --- |
| `tantrix-game` | Rules, tiles, board, turns, scoring, history, undo/replay; no UI or I/O |
| `tantrix-ai` | `TantrixAi` implementations, evaluation, seeded strategies |
| `tantrix-tui` | Terminal game |
| `tantrix-tournament` | TOML-configured AI round robin; TUI or headless |
| `tantrix-app` | Dioxus UI library, `tantrix` binary, Axum/SQLite backend |
| `tantrix-proto` | JSON client/server messages and snapshots |
| `tantrix-net` | Browser/native WebSocket sessions |
| `tantrix-room` | Shared room engine, rules, handshake, session cookies |
| `tantrix-worker` | Cloudflare SSR and room Durable Object |

Other paths:

| Path | Contents |
| --- | --- |
| `tantrix-app/assets/` | CSS and favicon; CSS assembled in `src/lib.rs` |
| `tantrix-app/public/` | Web manifest and icons |
| `tantrix-game/doc/game_rules.md` | Game rules |
| `doc/` | Domain documentation; screenshots/header in `doc/assets/` |
| `cloudflare/` | Worker JS entry, Wrangler configuration |
| `scripts/` | Worker build, release packaging, local tunnel |
| `e2e/` | Playwright tests for both backends |
| `vendor/dioxus-server/` | Dioxus 0.8.0-alpha.3 wasm32 patch; see `PATCHES.md` |

## Commands

Run from the workspace root. Recipe definitions: `justfile`; setup: [doc/development.md](doc/development.md).

```sh
just dev                       # Web + native backend; hot reload/logs
just run                       # Release web + native backend
just desktop                   # Desktop development
cargo run -p tantrix-tui
cargo run --release -p tantrix-tournament -- --config tantrix-tournament/configs/championship.toml --headless
just fmt-check
just lint
just test                      # Workspace + app server tests
cargo test -p tantrix-app       # App only; add --features server for server tests
just test-e2e                  # Native backend browser tests
just worker-e2e                # Build Worker + browser tests
just worker-deploy             # Build + deploy Worker
```

Worker build steps are private prerequisites of the public recipes. The app binary is `tantrix`: client output is `target/dx/tantrix/release/web/public`, not `target/dx/tantrix-app/...`.

## App map

Paths below are relative to `tantrix-app/src/`.

| Path | Responsibility |
| --- | --- |
| `lib.rs`, `main.rs` | App root/styles/router; platform entry |
| `route.rs` | `/`, `/learn`, `/about`, `/game?seed=N`, `/online`, `/online/game/:id`, `/join/:code`, fallback; `/download` except desktop/native |
| `state/` | Data/actions, no RSX: game/UI state, driver, AI, gestures/drag, online sessions, downloads, desktop settings/updates |
| `hooks/` | Context providers, camera, derived game/phase/scoring values, online snapshots, downloads/updates |
| `pages/game/` | Shared local/online HUD, hands, drag overlay, results; render-count tests |
| `pages/local_game/`, `pages/online/` | Local setup/play; online entry/lobby/room |
| `components/board/`, `components/tile/` | Board layers, viewport, controls, rotation picker; SVG tiles |
| `components/` | Reusable UI; no session access |
| `svg/` | Pure geometry, paths, camera math, card placement |
| `server.rs`, `server/` | Axum routes, room sockets, SQLite storage |
| `desktop.rs` | Desktop window chrome/platform integration |
| `util/test_page.rs` | Test-only `GamePage` harness |

## Domain and authority

- `Game` owns game state; mutate through its API. `GameState` adds cached `Game::moves_by_tile()` results (`hand_info`).
- `Placement` = tile + axial position + rotation; `Move` adds the rules' forced flag. `Board` stores up to 56 moves, sorted by position. Each tile has three of four colors; rotations step by 60°.
- `TurnEvent` records moves, skips and retirements for undo/replay. Local games/tournaments retain reproducible seeds; online rooms use random deals and persist `GameData`. Never send a seed or bag order to players.
- `RoomEngine` takes socket events and produces state/effects. Both backends persist before committing state and delivering effects.
- `GameCtx` = atomic `Signal<GameState>` + field-reactive `Store<UiState>` + `GameDriver`. `Local` runs turns/AI here; `Online(RoomLink)` sends intents and mirrors authoritative snapshots. Components query the driver; actions dispatch through it.
- `TurnPhase`: SelectTile → SelectPosition → SelectRotation, plus WaitingForAi/GameOver.

## Required conventions

- Keep rules in `tantrix-game`; edge matching in `rules.rs`. Touching colors must match. Forced spaces (3+ neighbors) take priority when a hand tile fits. Bag-dependent restrictions: no same-color triples, four-sided spaces or controlled-side plays.
- Component dependency order: page → organism → molecule → atom. Pages/organisms belong in `pages/`; reusable molecules/atoms in `components/`. Lower tiers never import upper tiers.
- Props: plain `T` by default, `ReadSignal<T>` for narrow subscriptions, `EventHandler<T>` for callbacks. No mutable `Signal<T>` props.
- Read reactive state in the smallest RSX scope or a memo, not at component top. Use `Store` for independently changing fields, `Signal` for atomic data. Write store fields individually or through `#[store]` methods; never `ui_state.write()` or writes during rendering. Use `set_if_changed` for potentially unchanged fields.
- Put `key` on each RSX loop's direct child; a block returning `rsx!` leaves the list unkeyed. Split components by subscriptions; memoize expensive derivations; avoid cloning large state. Profile before optimizing.
- One component tree across screen sizes; responsiveness is CSS only. Board fills the backdrop; HUD/hands float above it. `game.css` sets base layout, `desk.css` expands it; camera frames the remaining stage and derives home zoom from stage size.
- SVG tiles/paths use axial-to-pixel geometry. Theme: global `THEME`, OKLCH CSS variables. Shared tokens/components live in `tokens.css`/`components.css`; desktop chrome in `desktop.css`.
- Production client build rebuilds std with silent immediate-abort panics (see `justfile`). Preserve dx symbols for hydration; never add `-Zfmt-debug=none` (`Color` display, keys and data attributes need `Debug`). Measure Wasm with `twiggy top` on a `dx build --keep-names` copy.

## Tests

Keep tests beside code and deterministic: seeded games/fixed data, no timers. Extract decisions into pure functions. For actions/hooks/driver tests use `util::test_page::GamePage`, `state::test_games` and `state::online_state::test_rooms`. `act` catches up the page; `with` does not, so AI timers do not start. Render counts: `util::render_count` and `pages/game/rerender_tests.rs`.

## Domain docs

[Engine](doc/game.md) · [AI/tournaments](doc/ai.md) · [Multiplayer](doc/MULTIPLAYER.md) · [Components](doc/components.md) · [State](doc/signals.md) · [Styling](doc/styling.md) · [Worker](cloudflare/README.md) · [Browser tests](e2e/README.md) · [Desktop releases](doc/desktop-release.md) · [Android releases](doc/android-release.md) · [Updates](doc/desktop-updates.md)

## Skills

Read the matching `.codex/skills/<name>/SKILL.md` before the work below.

| Skill | Trigger |
| --- | --- |
| `dioxus-components` | Components, RSX, props, events |
| `dioxus-state` | Reactive data ownership, Signal vs Store |
| `dioxus-hoisting` | Component splits, lifted state, prop types/callbacks |
| `dioxus-spawn` | Background futures/event tasks, unexpected cancellation |
| `dioxus-async` | Resources, suspense, server functions |
| `dioxus-routing` | Routes/navigation |
| `dioxus-styling` | CSS, themes, assets, rendering performance |
| `dioxus-cli` | Builds, CLI/configuration, deployment |
| `documentation` | Documentation, rules, skills |
