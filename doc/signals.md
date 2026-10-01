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
| `phase` | StatusBar, `use_phase_view`, `use_is_game_over`, routing |
| `message` | StatusBar |
| `selected_tile`, `selected_position` | `use_phase_view`, HandTile, actions |
| `hovered` (enum) | `use_phase_view`, `use_highlight_map`, RotationWheel |
| `zoom`, `pan_x`, `pan_y`, `drag` | `use_board_camera` only |
| `new_tile` | PlayerHand (just-drawn animation) |
| `last_placed_position` | BoardTilesLayer (just-placed animation) |
| `highlighted_color` | `use_highlight_map`, ScoresPanel |

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
| `use_board_camera` | `(BoardCamera, BoardCameraHandlers)` | camera fields |
| `use_phase_view` | `Memo<PhaseView>` | phase, selection, hover, `game_state` |
| `use_highlight_map` | `Memo<BTreeMap<Position, Vec<Color>>>` | phase, board, highlight/hover color |

## Reading rules

- **Read narrowly.** Field-level where possible — `ui.phase()()`, not
  `ui.read().phase`.
- **Derived state → `use_memo`.** Never a shadow `Signal` you hand-sync.
- **Lift derived per-child state into the child**, not out to the parent's
  render loop. Example: `BoardTile` reads `use_highlight_map` itself,
  so only the affected tile re-renders when the highlight changes.
- **Don't hold `.read()` across `.await`.** Copy scalar out first.

## Writing rules

- **Single field write:** `ui.phase().set(…)`, `ui.hovered().set(…)`.
  Marks only that field dirty.
- **Multi-field write:** `ui.write().reset_selection()`. Marks every
  field dirty — correct here because all of them are changing.
- **`after_move`** is the single "a move landed" hook in `UiState`: resets
  pan, records `new_tile` + `last_placed_position` for animations. Called
  from both the human (`execute_move`) and the AI (`run_ai_loop`) paths.

## Turn flow (where the writes happen)

```
Click hand tile    → click_tile → select_tile (or auto-play via execute_move)
Click position     → click_position → execute_move OR phase = SelectRotation
Pick rotation      → pick_rotation → execute_move
execute_move       → Local:  game_state.write().play_move
                             ui_state.write().after_move
                             advance_turn
                     Online: RoomLink::play (PlayMove) + reset selection
advance_turn       → (Local only) if next player is AI:
                         phase = WaitingForAi
                         the page's AI-turn effect spawns run_ai_loop + settle_turn
                     else: settle_turn directly
run_ai_loop        → loops: ai_choose → animate selection → play_move
settle_turn        → skip stuck players, set phase/message, reset selection
End Game           → end_game → Local: phase = GameOver
                                Online: RoomLink::end_game (EndGame)
```

All turn actions live in `state/actions.rs` and take a single `GameCtx`.
The AI loop is spawned from the game page's scope (the effect is installed
by `use_game_ctx`) so transient tile/hex unmounts do not cancel it, while
leaving the page still does.
