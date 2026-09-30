# Game Implementation

Four crates, clear dependency arrows. Rules live in `tantrix-game`; UI
crates never reach upstream.

```
tantrix-game  ─┐
               ├→ tantrix-app  (Dioxus web/desktop/mobile)
tantrix-ai   ──┤
               └→ tantrix-tui  (ratatui)
```

## Crates

| Crate | Purpose |
|---|---|
| `tantrix-game` | Rules, board, tiles, moves, scoring. No UI, no async. |
| `tantrix-ai` | AI players (`SmartAi`, `RandomAi`, …) behind a `TantrixAi` trait |
| `tantrix-app` | Dioxus frontend |
| `tantrix-tui` | ratatui terminal frontend |

## Domain (from `tantrix-game`)

| Type | Notes |
|---|---|
| `Tile` | 56 unique hexagons; each has 3 colored edge-pairs |
| `Color` | Red / Blue / Yellow / Green |
| `Position` | Axial `(i8, i8)`. Center is `(0, 0)` |
| `Direction` | 6 edges |
| `Rotation` | 60° steps: `None`, `R60`, …, `R300` |
| `Placement` | `Tile + Position + Rotation`: what a player asks to play |
| `Move` | `Placement + forced: bool`; the rules set `forced` (legal moves, board contents) |
| `Board` | `ArrayVec<Move, 56>` sorted by position |
| `Hand` | 6 tiles per active player |
| `Bag` | Remaining tiles in the shared draw pile |

## Rules

- **Golden rule.** Touching edges must match color. Enforced by
  `Board::possible_moves` (`rules.rs`) plus the checked `Game::play_move`
  mutation path, which takes a `Placement` and places the legal `Move`
  that matches it. `Board::insert_move` is a low-level occupancy-only
  insertion helper.
- **Forced space.** Empty hex with 3+ filled neighbors. If any forced
  move is possible, the player *must* play one.
- **Restrictions** (while `bag` is non-empty):
  - No same-color triples (would create an unfillable forced space).
  - No controlled-side plays.
  See `Board::possible_moves` for the full filter chain.

## Turn state machine

```
SelectTile  ──click tile──▶  SelectPosition
SelectTile  ◀───────────── reset_selection
SelectPosition  ──single rotation / empty board──▶  (execute_move) ──▶ SelectTile
SelectPosition  ──multiple rotations──▶  SelectRotation
SelectRotation  ──pick rotation──▶  (execute_move) ──▶ SelectTile
any phase       ──game ends──▶  GameOver
non-human turn  ────────────▶  WaitingForAi
```

`TurnPhase` lives in `state/ui_state.rs`. Routing between `PlayerSetup`,
`GamePlay`, and `GameOver` matches on `(Screen, TurnPhase)` in `LocalGame`.

## Human turn

All in `state/actions.rs`, each action takes `GameCtx`:

```
click hand tile  → click_tile        select, or auto-play if only 1 move
click position   → click_position    execute_move, or → SelectRotation
pick rotation    → pick_rotation     execute_move
execute_move     → game_state.write().play_move(Placement { tile, position, rotation })
                 → ui_state.write().after_move    (new_tile + pan reset)
                 → advance_turn
```

## AI turn

`advance_turn` checks `Player::Ai(_)` for the current color. If AI, it
sets `phase = WaitingForAi`. `LocalGame` owns a `use_effect` that reacts
to that phase and spawns the loop from the page scope, so the task is
cancelled if the page unmounts but not when a clicked tile/hex disappears.

`state/ai_service.rs::run_ai_loop` iterates while the current player is
still an AI:

```
read legal_moves (forced-filtered if any forced exist)
if no moves → skip_stuck_players          stop if a whole round can't move
ai_choose()                               pick best move
find_tile_index in the AI's hand
ui.selected_tile = idx
sleep 500ms                               show tile pick
ui.selected_position = chosen.position
sleep 400ms                               show destination
game_state.write().play_move(chosen.placement())   commit
ui update (new_tile, last_placed_position, reset selection, reset pan)
sleep 300ms                               hand-draw animation
```

When the loop exits (next player is human or game over), `settle_turn`
runs: skip stuck players, set message, reset selection.

## Forced moves

A turn is: fill forced spaces, make one free move, fill forced spaces
again. `TurnTracker` records the step (`TurnStep::BeforeFreeMove` /
`AfterFreeMove`); online snapshots carry it as `turn_step`.

- `Board::possible_moves` retains only `forced` moves if any exist, and
  only forced moves at all once the free move is played.
- Both the human (`execute_move`) and the AI are given only forced moves
  in that state — can't accidentally play a non-forced one.
- `Game::play_move` is the one place a move is played (the app's
  `GameState::play_move` wraps it). Callers pass a `Placement` — tile,
  position, rotation — and never a `forced` flag: the legal `Move` that
  matches it says whether it fills a forced space. An illegal placement is
  `MoveError::InvalidMove`, a tile the player doesn't hold `TileNotInHand`.
  It passes the turn only after the free move, once no forced space is left
  that the player can fill, and returns the `PlayedMove` as placed.

## Stuck players

`Game::skip_stuck_players` is the one routine that passes the turn over
players with no legal move. It checks each player at most once and returns
`true` when nobody can move although tiles remain (a stall). `Game::advance`
(TUI, tournament), the room's `PlayMove` / `SkipTurn` / `StartGame`, and
the app's `settle_turn` and AI loop all use it, so an all-AI game where
everyone is stuck ends instead of spinning.

## Scoring & game over

- Game ends when `bag` is empty AND every hand is empty.
- Each color's score = longest line or loop of that color — see
  `Board::scoring_paths` / `Board::scores`.
- `Game::winners` returns every playing color tied for the top score
  (the room's `EndReason`, the app's `ScoresPanel`).
- Entering `TurnPhase::GameOver` switches the screen to `GameOver` (via
  `LocalGame`'s routing match), which shows `Board` (read-only, scoring
  overlays on) beside `ScoresPanel`.

## AI service

`tantrix-ai` exposes the `TantrixAi` trait:

```rust
trait TantrixAi {
    fn name(&self) -> &str;
    fn choose_move(&self, board: &Board, hands: &EnumMap<Color, Hand>,
                   current: Color, possible_moves: &[Move]) -> Move;
    fn clone_box(&self) -> Box<dyn TantrixAi>;
}
```

An AI picks one of the legal `possible_moves`, so it returns a `Move`;
callers play its `placement()`.

Implementations: `RandomAi`, `FirstMoveAi`, `GreedyAi`, `DefensiveAi`,
`LoopHunterAi`, `SmartAi`. Pick per player in `state/config.rs`
(`player_for`).

## Frontend cache

`Game` is the single source of truth: it keeps the current player's
`legal_moves` up to date after every change, and `Game::moves_by_tile`
groups them per hand tile as `TileMoveInfo` (positions → valid rotations +
`has_forced`; the TUI uses it too). The app's `GameState` wraps a `Game`
(local deal via `Game::from_players_with_rng`, online mirror via
`Game::from_snapshot`) and caches `hand_info = moves_by_tile()`, refreshed
inside `play_move` / `skip_stuck_players`. It derefs to `Game` for reads,
so components use `gs.hand(color)`, `gs.current_player()`, `gs.board()`…
and `gs.hand_info()` instead of regrouping moves each render.
