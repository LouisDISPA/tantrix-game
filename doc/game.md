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
ai_choose()                               pick best move (and its index in the hand)
ui.selected_tile = idx
sleep 500ms                               show tile pick
ui.selected_position = chosen.position
sleep 400ms                               show destination
game_state.write().play_move(chosen.placement())   commit
ui update (new_tile = PlayedMove::hand_index, last_placed_position, reset selection, reset pan)
sleep 300ms                               hand-draw animation
```

When the loop exits (next player is human or game over), `settle_turn`
runs: skip stuck players (writing `game_state` only if the current player
`Game::is_stuck`), set message, reset selection.

## Forced moves

A turn is: fill forced spaces, make one free move, fill forced spaces
again. `TurnTracker` records the step (`TurnStep::BeforeFreeMove` /
`AfterFreeMove`); online snapshots carry it as `turn_step`.

- `Board::possible_moves` retains only `forced` moves if any exist, and
  only forced moves at all once the free move is played. It works out each
  contour position's neighbor colors, forced flag, controlled side and
  same-color-triple colors once per call, and tries forced spaces first;
  the move-by-move rules (`get_possible_move`, `controlled_positions`,
  `creates_same_color_triple`) stay as the reference its tests compare
  against, move order included.
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
players with no legal move (`Game::is_stuck`: no legal move, game not
over). It checks each player at most once and returns `true` when nobody
can move although tiles remain (a stall). `Game::advance` (TUI,
tournament), the room's `PlayMove` / `SkipTurn` / `StartGame`, and the
app's `settle_turn` and AI loop all use it, so an all-AI game where
everyone is stuck ends instead of spinning.

## Scoring & game over

- Game ends when `bag` is empty AND every hand is empty.
- Each color's score = longest line or loop of that color — see
  `Board::scoring_paths` / `Board::scores`, one walk over every path
  (`for_each_path`).
- `Board::scores_after(scores, m)` gives the scores once a legal move `m`
  is placed without copying the board: paths only grow, so each color's
  line through `m` is compared with its old best (a move that closes a
  loop is scored on a copy). The AIs score every candidate this way, from
  the board's scores worked out once per decision.
- `Game::winners` returns every playing color tied for the top score
  (the room's `EndReason`); `winners_from(scores)` does the same from
  scores already worked out (the app's `ScoresPanel`, which scores the
  board once for every row).
- Entering `TurnPhase::GameOver` switches the screen to `GameOver` (via
  `LocalGame`'s routing match), which shows `Board` (read-only, scoring
  overlays on) beside `ScoresPanel`. `BoardTilesLayer` builds the scoring
  overlay (`use_highlight_map`: the paths once per game over, filtered by
  the hovered or locked color) and hands each `BoardTile` its colors.

## History, undo and replay

The board is sorted by position and draws only shrink the bag, so `Game`
records every change in order as a `TurnEvent` (`history.rs`), read with
`Game::history()`:

| Event | Recorded by | Holds |
|---|---|---|
| `Played` | `play_move` | player and `TurnStep` before the move, the `Move` as placed, the `hand_index` the tile left, the tiles `drawn` (in draw order) |
| `Skipped` | `skip_turn` (so `skip_stuck_players`) | player and `TurnStep` |
| `Retired` | `retire_color` | the color |

- **Undo.** `Game::undo()` reverts the last event exactly and returns it:
  the tile leaves the board, the drawn tiles go back on top of the bag, the
  played tile goes back to `hand_index`, and the turn goes back to the
  recorded player and step; legal moves are refreshed. It returns `None`
  when there is nothing to undo, or when the last event is a retirement
  (the retired hand is gone). It undoes one event: skips and AI moves
  count too.
- **Seeds.** `Game::new` / `from_players` deal from a random seed,
  `new_seeded` / `from_players_seeded` from a given one; either way
  `Game::seed()` keeps it. They deal exactly like `from_players_with_rng`
  with `Xoshiro256PlusPlus::seed_from_u64(seed)` (which keeps no seed), so
  `/game?seed=N` and the e2e fixture are unchanged.
- **Replay.** `Game::replay(seed, players, events)` deals from the seed and
  plays each event again with `Game::replay_event`, which checks it happens
  exactly as recorded (same player and step; for a move the same placement,
  hand index and draws) and otherwise leaves the game unchanged and returns
  an `EventError` (`ReplayError` says which event). `replay_event` also
  redoes what `undo` returned.
- **Persistence.** `GameData` (`to_data` / `from_data`) carries the seed (if
  any) and history; rooms saved before them load with none. The room's
  `StartGame` deals with `from_players_with_rng` and the OS-seeded thread
  RNG, so the hidden bag order is not limited to a 64-bit seed through a
  non-cryptographic generator, and server games keep no seed. They need
  none: `GameData` keeps the bag and the full history, and undoing the
  history gives back the deal (which the history then replays exactly).
  `GameSnapshot` carries neither the bag, the seed nor the history.

## AI service

`tantrix-ai` exposes the `TantrixAi` trait:

```rust
trait TantrixAi: Send + Sync {
    fn name(&self) -> &str;
    fn clone_box(&self) -> Box<dyn TantrixAi>;
    fn with_seed(&self, seed: u64) -> Box<dyn TantrixAi>;  // default: clone_box
    fn choose_move(&self, board: &Board, hands: &EnumMap<Color, Hand>,
                   current_player: Color, active_players: &[Color],
                   possible_moves: &[Move]) -> Move;
}
```

An AI picks one of the legal `possible_moves`, so it returns a `Move`;
callers play its `placement()`.

Implementations: `RandomAi`, `FirstMoveAi`, `GreedyAi`, `DefensiveAi`,
`LoopHunterAi`, `SmartAi`. Pick per player in `state/config.rs`
(`player_for`). All but `RandomAi` are deterministic (ties go to the last
best move in legal-move order). `RandomAi::new()` draws from the thread's
RNG; `RandomAi::seeded(seed)` (or `with_seed`) owns a seeded RNG, and a
copy carries on from the same state.

`tantrix-tournament [SEED]` plays every pairing of `all_ais()`, the first
AI as Red and the second as Blue, and scores each AI by its color. Each
game's seed is drawn in order from the tournament seed before any game
runs, and deals the game and seeds both AIs (`with_seed`), so a tournament
gives the same results however rayon schedules its games. Only those seeds
are kept up front (`game_seeds`); each game's pairing comes from its index
(`pairing`) as it is played, and the leaderboard is sorted once per frame.

`cargo run --release -p tantrix-ai --example seeded_games [GAMES_PER_PAIR]`
times seeded games between every pairing (then the rules and the scoring
alone) and prints a checksum of their histories and scores: a change that
must not alter play leaves it unchanged.

## Frontend cache

`Game` is the single source of truth: every change clears the current
player's `legal_moves`, and the first read after it works them out once
(a `Game` rebuilt from `GameData` only to be read or saved, as the room
does for every snapshot, never computes them). `Game::moves_by_tile`
groups them per hand tile as `TileMoveInfo` (positions → valid rotations +
`has_forced`; the TUI uses it too). The app's `GameState` wraps a `Game`
(local deal via `Game::from_players_seeded` / `from_players`, online mirror
via `Game::from_snapshot`) and caches `hand_info = moves_by_tile()`, refreshed
inside `play_move`, and inside `skip_stuck_players` when it skips someone.
It derefs to `Game` for reads, so components use `gs.hand(color)`,
`gs.current_player()`, `gs.board()`… and `gs.hand_info()` instead of
regrouping moves each render.
