# Signals & State

Reactive state lives in three shapes: `GlobalSignal` (app-wide), `Signal<T>`
(atomic), and `Store<T>` (field-level). Choose by how writes arrive — see
`dioxus-state` skill for the full decision guide.

## Inventory

| State | Shape | Where | Why |
|---|---|---|---|
| `THEME` | `GlobalSignal<Theme>` | `state/theme.rs` | App-wide, outlives any page |
| `screen` | `Signal<Screen>` (`Setup` \| `Playing`) | `state/ui_state.rs` | One value, simple routing switch |
| `game_state` | `Signal<GameState>` | `state/game_state.rs` | Every `play_move` touches many fields — atomic |
| `ui_state` | `Store<UiState>` | `state/ui_state.rs` | ~10 fields change independently; readers want narrow subscriptions |

The last three live in a shared `GameCtx` (`Copy` struct), provided by
`LocalGame` and consumed by every component under it.

```rust
pub struct GameCtx {
    pub screen:     Signal<Screen>,
    pub game_state: Signal<GameState>,
    pub ui_state:   Store<UiState>,
}
```

## Field maps

### `GameState` (atomic — read as a whole)

`board`, `bag`, `hands`, `players`, `turn_tracker`, `hand_info`.
Accessed as `ctx.game_state.read().board.len()`.

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

## Derived values — `hooks/`

Each hook wraps a `use_memo` over `GameCtx`. Readers call them directly;
no one maintains hand-synced signals.

| Hook | Returns | Deps |
|---|---|---|
| `use_placed_moves` | `Memo<Vec<Move>>` | `game_state.board` |
| `use_current_player` | `Memo<Color>` | `turn_tracker` |
| `use_is_game_over` | `Memo<bool>` | `ui_state.phase` |
| `use_has_forced_move` | `Memo<bool>` | `game_state.hand_info` |
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
execute_move       → game_state.write().play_move
                   → ui_state.write().after_move
                   → advance_turn
advance_turn       → if next player is AI:
                         phase = WaitingForAi
                         spawn_forever(run_ai_loop + settle_turn)
                     else: settle_turn directly
run_ai_loop        → loops: ai_choose → animate selection → play_move
settle_turn        → skip stuck players, set phase/message, reset selection
```

All turn actions live in `state/actions.rs` and take a single `GameCtx`.
The AI loop must use `spawn_forever` (not `spawn`) — see `dioxus-spawn`.
