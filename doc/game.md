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
| `tantrix-ai` | AI players (`SmartAi`, `MonteCarloAi`, …) behind a `TantrixAi` trait |
| `tantrix-app` | Dioxus frontend |
| `tantrix-tui` | ratatui terminal frontend |
| `tantrix-tournament` | AI round robin: TUI or headless, AIs set by TOML configs ([ai.md](ai.md)) |

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
- **Forced space.** Empty hex with 3+ filled neighbors. The player must
  fill one if possible, including after the bag empties.
- **Restrictions** (while `bag` is non-empty):
  - No same-color triples (would create an unfillable forced space).
  - No fourth tile around a forced space.
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
`GamePlay`, and `GameOver` matches on the `Screen` and whether the phase is
`GameOver` (a memo) in `LocalGame`. Start Game calls `actions::start_game`:
it deals the configured game and hands the first turn on (to the AI loop
when an AI seat starts), so `GamePlay` itself writes nothing while it
renders;
`OnlineGameView` shows `GamePlay` until the room's game has ended, then
`GameOver`. Both screens live in `pages/game/` and read the page's
`GameCtx`, whose `GameDriver` says who plays: `Local` (every seat on this
device) or `Online` (a room; this device only sends its seat's intents).

## Human turn

All in `state/actions.rs`, each action takes `GameCtx`:

```
click hand tile  → click_tile        deselect it, auto-play if only 1 move, or select
click position   → click_position    execute_move, or → SelectRotation
pick rotation    → pick_rotation     execute_move
execute_move     → Local:  game_state.write().play_move(Placement { tile, position, rotation })
                           ui_state.after_move    (new_tile + pan reset, per field)
                           advance_turn
                   Online: RoomLink::play → SessionCommand::PlayMove (own seat only);
                           the room's next snapshot redraws the board
```

A hand tile is clickable only on its seat's turn and when
`GameDriver::controls(color)`: any seat locally, only this device's own
seat online. `HandTile` offers the click only then, and `click_tile`
checks the seat again.

A move needs no choice when `auto_move` finds one: the tile fits at a
single position in a single rotation (`auto_rotation`) — or, on the empty
board, where the opening tile fits every way round at the centre, in the
first. `click_tile` plays it straight away, `click_position` plays the one
rotation of a hex, and the hover preview shows a hand tile's only move
(not the opening tile's: its rotation is the click's to pick).

## AI turn

`advance_turn` (local games only: `GameDriver::runs_turns`) first skips the
players who cannot move, then checks the player to move: an AI with a legal
move gets `phase = WaitingForAi`, anyone else `settle_turn`. The effect
`use_game_ctx` installs on the game page reacts to that phase and spawns
the loop from the page scope, so the task is cancelled if the page unmounts
but not when a clicked tile/hex disappears.

`state/ai_service.rs::run_ai_loop` iterates while the current player is
an AI that can move:

```
ai_choose()                               pick best move (and its index in the hand);
                                          None for a human, an AI with no move, game over
ui.selected_tile = idx
sleep 500ms                               show tile pick
ui.selected_position = chosen.position
sleep 400ms                               show destination
game_state.write().play_move(chosen.placement())   commit
ui_state.after_move (new_tile = PlayedMove::hand_index, last_placed_position, reset pan),
  clear selected tile + position          per-field writes: the camera and the
                                          other fields' readers stay asleep
sleep 300ms                               hand-draw animation
```

When the loop exits (next player is a human, a stuck AI, or the game is
over), `advance_turn` runs again: it skips stuck players (writing
`game_state` only if the current player `Game::is_stuck`) and hands the
turn to the loop again if an AI seat is next, or `settle_turn` sets the
message and resets the selection for a human (or ends the game).

## Forced moves

A turn is: fill forced spaces, make one free move, fill forced spaces
again. This still applies after the bag empties; only the three
restriction rules are lifted. `TurnTracker` records the step (`TurnStep::BeforeFreeMove` /
`AfterFreeMove`); online snapshots carry it as `turn_step`.

- `Board::possible_moves` retains only `forced` moves if any exist, and
  only forced moves once the free move is played. With an empty bag,
  forced spaces still take priority while restrictions lift. It works out each
  contour position's neighbor colors, forced flag, controlled side and
  same-color-triple colors once per call, looking positions up in a dense
  `Grid` of the board's bounding box (`grid.rs`) rather than searching the
  sorted board, checks tiles against packed `Edges` (two bits per
  direction: a whole rotation in one compare, `edges.rs`; the golden-rule
  checks on them stay in `rules.rs`), and tries forced spaces first;
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
  It passes the turn after the free move once no forced space is left
  that the player can fill, including with an empty bag, and returns
  the `PlayedMove` as placed.

## Stuck players

`Game::skip_stuck_players` is the one routine that passes the turn over
players with no legal move (`Game::is_stuck`: no legal move, game not
over). It checks each player at most once and returns `true` when nobody
can move although tiles remain (a stall). `Game::advance` (TUI; the
tournament runs the same loop, timing each choice), the room's `PlayMove` and `StartGame`, and the app's
`advance_turn` all use it, so an all-AI game where everyone is stuck ends
instead of spinning. `advance_turn` is the app's only caller, for
a human's move and after each AI turn alike, so a skip that lands on an AI
seat always starts the AI loop.

## Scoring & game over

- Game ends when `bag` is empty AND every hand is empty.
- Each color's score = longest line or loop of that color — see
  `Board::scoring_paths` / `Board::scores`, one walk over every path
  (`for_each_path`).
- `Board::scores_after(scores, m)` gives the scores once a legal move `m`
  is placed without copying the board: paths only grow, so each color's
  line through `m` is compared with its old best (a move that closes a
  loop is scored on a copy). The AIs score every candidate this way, from
  the board's scores worked out once per decision, on `Board::indexed()`
  (an `IndexedBoard`: the board with a position grid, built once for all
  the candidates). `score_after(score, m, color)` scores one color alone:
  a loop outscores the line it closes, so it is exact without a copy.
- `Board::lines()` lists every line and loop, each line with the empty
  positions its ends lead into (`LineEnd`): the planning AIs' evaluation.
- `Game::winners` returns every playing color tied for the top score
  (the room's `EndReason`); `winners_from(scores)` does the same from
  scores already worked out (the app's `ScoresPanel`, which scores the
  board once for every row).
- Entering `TurnPhase::GameOver` switches the screen to `GameOver` (via
  `LocalGame`'s routing match; online, the room's `GameEnded`), which shows
  `Board` (read-only, scoring overlays on) beside `ScoresPanel` and the
  exits: Play Again (local) or Online Menu (online), and Main Menu. End
  Game calls `actions::end_game`: locally it ends the game here, online it
  asks the room to end it for everyone. `BoardTilesLayer` builds the scoring
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

`tantrix-game` defines the `TantrixAi` trait (`player.rs`); `tantrix-ai`
implements it:

```rust
trait TantrixAi: Send + Sync {
    fn name(&self) -> &str;
    fn clone_box(&self) -> Box<dyn TantrixAi>;
    fn with_seed(&self, seed: u64) -> Box<dyn TantrixAi>;  // default: clone_box
    fn choose_move(&self, view: &AiView) -> Move;
}
```

`Game::ai_view` shows an AI what a player sees: the board, every hand (face
up), the current player, the playing colors in turn order, the turn step,
the bag count and the legal moves (never empty), but not the draw order.
`AiView::unseen_tiles` lists the tiles neither on the board nor in a hand,
and `AiView::determinize(rng)` deals them into a bag of its own: a `Game`
(every seat `Player::Someone`) to play on from here. An AI returns one of
`legal_moves`; callers play its `placement()`.

The AIs, how they choose, the tournament that ranks them and its results
are in [ai.md](ai.md). The app plays `SmartAi` (`state/config.rs`,
`player_for`); the TUI `RandomAi`.

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
