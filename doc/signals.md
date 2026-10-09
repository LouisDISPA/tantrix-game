# Signals & State

Reactive state lives in three shapes: `GlobalSignal` (app-wide), `Signal<T>`
(atomic), and `Store<T>` (field-level). Choose by how writes arrive — see
`dioxus-state` skill for the full decision guide.

## Inventory

| State | Shape | Where | Why |
|---|---|---|---|
| `THEME` | `GlobalSignal<Theme>` | `state/theme.rs` | App-wide, outlives any page |
| `screen` | `Signal<Screen>` (`Setup` \| `Playing`) | `LocalGame` (`Screen` in `state/ui_state.rs`) | One value, simple routing switch; local page only |
| `game_state` | `Signal<GameState>` | `state/game_state.rs` | Every `play_move` touches many fields — atomic |
| `ui_state` | `Store<UiState>` | `state/ui_state.rs` | ~10 fields change independently; readers want narrow subscriptions |

`game_state` and `ui_state` live in a shared `GameCtx` (`Copy` struct) with
the game's `driver`. Both game pages build it the same way, with
`hooks::use_game_ctx(driver, game)`, which provides it to every component
under the page (and runs the AI seats when the driver says they are played
here): `LocalGame` with `GameDriver::Local`, `OnlineGameView` (through
`use_online_game_ctx`) with `GameDriver::Online`.

```rust
pub struct GameCtx {
    pub game_state: Signal<GameState>,
    pub ui_state:   Store<UiState>,
    pub driver:     GameDriver,
}

pub enum GameDriver {
    Local,              // every seat is played on this device
    Online(RoomLink),   // a server-authoritative room
}
```

`GameDriver` (`state/game_driver.rs`) is plain `Copy` data: locally it holds
nothing; online its `RoomLink` holds the session's
`Signal<Option<CommandSender>>` and two memos over the `Store<OnlineState>`
— the seat this device plays (`my_color`) and each seat's nickname. Reading
`ctx.driver.controls(color)` / `my_color()` / `player_name(color)` inside
a memo subscribes to those memos only.

## Field maps

### `GameState` (atomic — read as a whole)

Wraps the core `Game` (board, bag, hands, players, turn, legal moves) plus
a `hand_info` cache. Reads go through `Game`'s accessors (`GameState`
derefs to it): `ctx.game_state.read().board().len()`,
`.current_player()`, `.hand(color)`, `.hand_info()`. Changes go through
`GameState::play_move` / `skip_stuck_players` so the cache stays in sync.

### `UiState` (field-level via `Store` derive)

| Field | Who reads |
|---|---|
| `phase` | GameHud (memos), the phase-view memos, `use_is_game_over`, LocalGame (memo), actions |
| `message` | GameHud |
| `selected_tile`, `selected_position` | the phase-view memos, HandTile, actions |
| `hovered` (enum) | `use_valid_positions` (SelectTile), `use_preview_placement`, `use_highlight_map` (game over) — never `use_orientations` |
| `zoom`, `pan_x`, `pan_y` | BoardViewport (transform memo), ZoomControls (zoom memo), OrientationPicker (while its card is open) |
| `gesture`, `dragged` | BoardViewport (dragging memo); `dragged` is only peeked, by click handlers |
| `stage` | the camera (peeked), OrientationPicker (while its card is open); written by `BoardCamera::stage_measured` when `Board` measures the stage (a camera still at its home zoom follows the new stage's) |
| `peeked` | `GamePlay`'s `shown` memo, and `GameHud`'s `viewing` memo (the desktop hint says whose tiles are looked at); cleared by an effect when the turn changes |
| `new_tile` | HandTile (just-drawn animation) |
| `last_placed_position` | BoardTilesLayer (just-placed animation, one memo for all tiles) |
| `highlighted_color` | `use_highlight_map`, ScoreRow |

Accessed as `ctx.ui_state.phase()()`. Never `ctx.ui_state.read()` —
that subscribes to every field.

### `GameDriver` (who plays)

| Method | Local | Online | Who asks |
|---|---|---|---|
| `runs_turns()` | `true` | `false` (the room plays turns) | `advance_turn`, the AI-turn effect |
| `controls(color)` | every seat | only `my_color` | HandTile |
| `can_end_game()` | `true` | only from a seat | EndGameButton |
| `my_color()` | `None` | the seat played here | PlayerHand ("you") |
| `player_name(color)` | `None` | the seat's nickname | PlayerHand |

`execute_move` and `end_game` match on it: locally they change
`game_state` / `ui_state`, online they send `PlayMove` / `EndGame` through
`RoomLink` and the room's next snapshot (fanned out by
`use_online_game_ctx`) drives the board.

## Derived values — `hooks/`

Each hook wraps a `use_memo` over `GameCtx`. Readers call them directly;
no one maintains hand-synced signals.

| Hook | Returns | Deps |
|---|---|---|
| `use_placed_moves` | `Memo<Vec<Move>>` | `game_state.board()` |
| `use_current_player` | `Memo<Color>` | `game_state.current_player()` |
| `use_is_game_over` | `Memo<bool>` | `ui_state.phase` |
| `use_has_forced_move` | `Memo<bool>` | `game_state.hand_info()` |
| `use_valid_positions` | `Memo<Vec<ValidPosition>>` | phase; hovered hand tile (SelectTile) or `selected_tile`; `hand_info` |
| `use_preview_placement` | `Memo<Option<Placement>>` | phase, selection, hover, `game_state` |
| `use_orientations` | `Memo<Option<Orientations>>` | phase, selection, `game_state` — not hover |
| `use_highlight_map` | `Memo<BTreeMap<Position, Vec<Color>>>` | phase, board, highlight/hover color |

`use_board_camera` returns a `BoardCamera` — the zoom/pan controls, which
write the camera fields — and reads nothing: BoardViewport and
ZoomControls memo the camera fields they show themselves.

The three phase-view memos used to be one `PhaseView` bundle that each
board layer recomputed itself, so a hover woke the orientation card. `Board`
now creates each once and passes it to the one layer that shows it as a
`ReadSignal` prop (`BoardEmptyLayer { positions }`, `BoardPreview {
placement }`, `OrientationPicker { orientations, placed }`): hovering a hex re-renders the
preview ghost and nothing else.

## Reading rules

- **Read narrowly.** Field-level where possible — `ui.phase()()`, not
  `ui.read().phase`.
- **Read in a memo or inline, not at the top.** A component that reads a
  signal while rendering re-renders on *every* write to it; reading a memo
  re-renders it only when the memo's value changes. `GameHud` shows
  memos (the hints, `bag_count`, the current player) so a hover, a drag
  or an AI picking its tile leaves it alone; `PlayerHand` memos its tiles
  and turn, so a move re-renders only the hands it changes.
- **Derived state → `use_memo`.** Never a shadow `Signal` you hand-sync.
- **One memo per derivation, at the common ancestor.** When several
  children (or a whole list) need a derived value, compute it once above
  them and pass it down: the tiles layer works out the scoring overlay and
  the last placed position once and gives each `BoardTile` its plain
  `highlight_colors` / `just_placed`, so a tile re-renders only when its
  own props change.
- **Key list items directly.** Put the keyed component itself in the `for`
  body (`for m in … { BoardTile { key: …, … } }`); a `{ let …; rsx! { … } }`
  block wraps each item in an unkeyed node, so the list is diffed by index
  and inserting a tile remounts every tile after it.
- **Don't hold `.read()` across `.await`.** Copy scalar out first.

## Writing rules

- **Single field write:** `ui.phase().set(…)`, `ui.hovered().set(…)`.
  Marks only that field dirty. `set_if_changed(ui.pan_x(), 0.0)` skips the
  write (and the wake-ups) when the field already holds the value.
- **Multi-field write:** the `#[store]` methods on `Store<UiState>`
  (`UiStateStoreImplExt`: `reset_selection`, `select_tile`, `go_back`,
  `clear_selection`, `after_move`) write field by field, only the fields
  that change. Never `ui.write()`: it marks *every* field dirty — the
  camera, the animation hints, the scoring overlay — and wakes all their
  readers. Only a full reset (`ui.set(UiState::default())` on Start Game
  and Play Again) writes the whole store.
- **Hot paths write the fewest fields.** A drag's pointer move writes
  `pan_x`, `pan_y` and `gesture` only; the AI loop leaves the phase alone (it
  already is `WaitingForAi`); an online snapshot resets the selection and
  `last_placed_position`, and frames the board (the camera's zoom and pan
  only) when a move arrived.
- **`after_move`** is the single "a move landed" change: resets pan,
  records `new_tile` + `last_placed_position` for animations. Called from
  both the human (`execute_move`) and the AI (`run_ai_loop`) paths.
- **No writes while rendering.** Turn hand-offs run from event handlers
  (`start_game` from Start Game) or effects, never from a component body.

## Turn flow (where the writes happen)

```
Start Game         → start_game → deal, reset UiState, advance_turn
Click hand tile    → click_tile → select_tile (or auto-play via execute_move)
Click position     → click_position → execute_move OR phase = SelectRotation
Pick rotation      → pick_rotation → execute_move
execute_move       → Local:  game_state.write().play_move
                             ui_state.after_move
                             advance_turn
                     Online: RoomLink::play (PlayMove) + reset selection
advance_turn       → (Local only) skip stuck players, then
                     if the player to move is an AI with a move:
                         phase = WaitingForAi
                         the page's AI-turn effect spawns run_ai_loop,
                         then advance_turn again
                     else: settle_turn
run_ai_loop        → loops while an AI can move:
                     ai_choose → animate selection → play_move
settle_turn        → set phase/message (GameOver if nobody can move),
                     reset selection (per field)
End Game           → end_game → Local: phase = GameOver
                                Online: RoomLink::end_game (EndGame)
```

All turn actions live in `state/actions.rs` and take a single `GameCtx`.
`advance_turn` is the one place the app skips stuck players, so every skip
hands the turn on the same way: to the AI loop when an AI seat is next.
The AI loop is spawned from the game page's scope (the effect is installed
by `use_game_ctx`) so transient tile/hex unmounts do not cancel it, while
leaving the page still does.
