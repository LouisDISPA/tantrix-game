# Tantrix Game

Rust implementation of the Tantrix board game — a hex-tile strategy game where 2-4 players compete to build the longest line or loop of their color. Three-crate workspace: core library, Dioxus web/desktop app, and a TUI.

## Tech Stack

- **Language**: Rust (nightly toolchain)
- **Frontend**: Dioxus 0.7 (web/desktop/mobile via feature flags)
- **TUI**: ratatui 0.30-alpha + crossterm
- **Build tool**: `dx` (Dioxus CLI)
- **Key crates**: enum-map, enum-iterator, arrayvec, rand, getrandom (wasm_js)

## Commands

```bash
# Dioxus web app (dev)
cd tantrix-app && dx serve --hot-reload

# TUI
cargo run -p tantrix-tui

# Rust checks (the same recipes CI runs)
just fmt-check
just lint
just test

# Browser e2e (Worker / native server)
just worker-e2e
just native-e2e

# Production build
cd tantrix-app && dx build --release

# Cloudflare Worker (SSR + hydration): build, run locally, deploy
just worker-build
just worker-dev
just worker-deploy
```

## Project Structure

```
tantrix-game/src/          # Core library — game rules, no UI
  lib.rs                   # Module list + crate-root re-exports
  tiles.rs                 # 56 Tile variants, Color enum (Red/Blue/Yellow/Green)
  coordinate_system.rs     # Position (axial hex), Direction, Rotation
  moves.rs                 # Move, PlayedMove, TileMoveInfo (legal moves per hand tile)
  board.rs                 # Board storage, lookups, contour, neighbor counts
  rules.rs                 # Legal moves: golden rule, forced spaces, restriction rules
  scoring.rs               # Longest line/loop per color: scores, scoring_paths
  bag.rs, hand.rs          # Bag (draw pile), Hand (6 tiles)
  turn.rs                  # TurnTracker, TurnStep
  player.rs                # Player, TantrixAi trait
  game.rs                  # Game (single source of truth: accessors, legal moves, play_move, skip_stuck_players, winners), GameData
  error.rs                 # MoveError, SetupError

tantrix-app/src/           # Dioxus frontend (lib + bin)
  lib.rs                   # `app()` root: stylesheet loading, Router
  main.rs                  # Binary entry: web client or native fullstack server
  route.rs                 # Route enum: MainMenu (/), LocalGame (/game)
  state/                   # Source-of-truth state (no RSX)
    game_state.rs          # GameState (wraps Game + hand_info cache) + GameCtx
    ui_state.rs            # UiState, TurnPhase, Hovered enum, HandTileState enum
    actions.rs             # Turn actions: execute_move, click_position, pick_rotation, advance_turn
    ai_service.rs          # Async AI turn loop
    config.rs              # GameConfig (player count/colors)
    theme.rs               # Global light/dark theme signal
  hooks/                   # Reactive derived values
    camera.rs              # use_board_camera (+ BoardCameraHandlers)
    derived.rs             # use_placed_moves, use_current_player, use_is_game_over, use_has_forced_move
    phase_view.rs          # use_phase_view → PhaseView, ValidPosition, WheelData
    scoring.rs             # use_highlight_map (scoring overlay)
  pages/                   # Route-level pages
    main_menu.rs           # Title screen
    local_game/            # Setup → Playing → GameOver screens
      screens/             # player_setup.rs, game_play.rs, game_over.rs
  components/              # Pure UI
    board/                 # Board composer + submodules
      mod.rs               # Board composes layers + chrome
      viewport.rs          # Zoom/pan host, children slot
      tiles_layer.rs       # Placed tiles + highlights
      empty_layer.rs       # Click targets + ghost preview
      controls.rs          # ZoomControls, EndGameButton
      rotation_wheel.rs    # Rotation picker
    tile/                  # Tile-shaped components
      hex_svg.rs           # Shared SVG primitive (polygon + curves)
      board_tile.rs        # Placed or preview tile on the board
      empty_hex.rs         # Empty hex slot (click target)
      hand_tile.rs         # Interactive hand tile
    player_hand.rs         # Hand display per player
    scores_panel.rs        # End-of-game scores card
    status_bar.rs          # Phase banner + counters
    title_decoration.rs    # Menu background decoration
  svg/                     # SVG geometry & path utilities
    geometry.rs            # axial_to_pixel, hex_vertices, bounding box
    paths.rs               # Color curves (arc/bezier/line), hex polygon

tantrix-tui/src/           # Terminal UI (ratatui)

tantrix-worker/src/        # Cloudflare Worker (workers-rs): SSR + Room Durable Object (room.rs, room_logic.rs)
cloudflare/                # Worker entry (index.js), wrangler.toml
vendor/dioxus-server/      # dioxus-server 0.7.10 patched for wasm32 (PATCHES.md)

assets/                    # CSS: main.css, board.css, hand.css, menu.css, animations.css
doc/                       # game_rules.md

tantrix-net/src/           # Browser WebSocket client for server-authoritative rooms
tantrix-proto/src/         # Client ↔ room wire messages (JSON)
e2e/                       # Playwright browser tests (see e2e/README.md)
```

## Domain Model

| Concept | Description |
|---------|-------------|
| **Tile** | One of 56 unique hexagons, each with 3 color pairs across 6 edges |
| **Color** | `Red`, `Blue`, `Yellow`, `Green` — each tile has exactly 3 colors |
| **Position** | Axial hex coordinate `(i8, i8)`, center is `(0,0)` |
| **Direction** | 6 hex edges: `Left`, `TopLeft`, `TopRight`, `Right`, `BottomRight`, `BottomLeft` |
| **Rotation** | 0°–300° in 60° increments: `None`, `R60`, `R120`, `R180`, `R240`, `R300` |
| **Move** | Tile + Position + Rotation + forced flag |
| **Board** | `ArrayVec<Move, 56>` of placed tiles, sorted by position |
| **Forced space** | Empty hex with 3+ neighbors — must be filled if possible |
| **Golden rule** | Touching edges must match in color |
| **Restriction rules** | No same-color triples, no 4-sided spaces, no controlled-side plays (while bag has tiles) |

## Code Conventions

- **Component tiers**: four tiers — global (page) → organism → molecule → atom. Atoms + molecules live in `components/*.rs`; organisms + globals in `pages/{feature}/*.rs`. Upper tiers use lower tiers; never the reverse. (See [rules/architecture.md](rules/architecture.md).)
- **State props**: plain `T` by default, `ReadSignal<T>` for granular subscriptions, `EventHandler<T>` for callbacks; mutable `Signal<T>` props are banned (see [rules/patterns.md](rules/patterns.md))
- **Reads**: inline in RSX or inside a `use_memo` — never `state.read()` at the top of a component
- **Store vs Signal**: `#[derive(Store)]` when fields change independently (`UiState`, `OnlineState`); plain `Signal<T>` when the struct is atomic (`GameState`)
- **Turn flow**: `TurnPhase` enum drives UI transitions (SelectTile → SelectPosition → SelectRotation)
- **GameState**: Wraps core `Game` (the single source of truth; reads via its accessors) with a `hand_info` cache of `Game::moves_by_tile()` (per hand tile: positions → valid rotations)
- **SVG rendering**: All board graphics are SVG with axial-to-pixel conversion; color curves use arc/bezier/line based on edge separation
- **Theme**: Global `THEME` store (light/dark), CSS custom properties with OKLCH colors
- **Layout**: CSS Grid adapts to player count (2–4 players, hands positioned left/right/top/bottom)
- **Naming**: PascalCase enums/structs, snake_case functions, UPPER_SNAKE_CASE constants

## Performance Rules

- **Key dynamic lists** — always add `key: "{id}"` to `for` loop items in RSX
- **Split large components** — break into smaller pieces so only changed parts re-render
- **Use `use_memo` for expensive derivations** — avoid recomputing on every render
- **Read signals narrowly** — read inside the smallest scope needed, not at component top
- **Avoid cloning large data** — use signals and borrowing instead
- **Debounce rapid input** — delay processing for search/filter inputs
- **Batch mutations** — use `extend()` / `with_mut()` instead of multiple individual pushes
- **Profile before optimizing** — use Chrome DevTools Performance tab, don't guess

## Critical Rules

- **Never break the golden rule** — all edge-matching logic lives in `tantrix-game/src/rules.rs`
- **Keep game logic in `tantrix-game`** — UI crates depend on it, never the reverse
- **Forced move priority** — when forced spaces exist and player has a fitting tile, it must be played

## Available Skills

| Skill | When to use |
|-------|-------------|
| `dioxus-components` | Creating or modifying Dioxus components, RSX, props, events |
| `dioxus-state` | Designing or refactoring reactive state: `Signal<T>` vs `Store<T>`, what to wrap at all, where it lives, how components consume it |
| `dioxus-hoisting` | Lifting shared state to a common ancestor, choosing prop types (`ReadSignal<T>` vs plain `T`), replacing mutable props with callbacks |
| `dioxus-spawn` | Launching futures with `spawn` / `spawn_forever` / `use_action`; diagnosing tasks that never run |
| `dioxus-async` | Reactive async data: `use_resource`, `SuspenseBoundary`, server functions |
| `dioxus-routing` | Adding or modifying routes and navigation |
| `dioxus-styling` | Styling components, CSS, assets, performance |
| `dioxus-cli` | CLI commands, Dioxus.toml config, hot-reload, bundling |

## When to Invoke Skills

- Creating a new component or page → `dioxus-components`
- Splitting a big component, choosing prop types (`ReadSignal<T>` vs plain `T`), or swapping a mutable prop for a callback → `dioxus-hoisting`
- Designing reactive state (signal vs plain, Signal vs Store, prop shapes) → `dioxus-state`
- Spawning background tasks from event handlers, or a task is mysteriously cancelled → `dioxus-spawn`
- Fetching async data, suspense, or adding server functions → `dioxus-async`
- Adding a new route or changing navigation → `dioxus-routing`
- Styling, theming, or asset management → `dioxus-styling`
- Build configuration, CLI issues, or deployment → `dioxus-cli`
