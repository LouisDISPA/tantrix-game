# State

Paths below are relative to `tantrix-app/src/`.

## Ownership

| State | Shape / owner | Purpose |
|---|---|---|
| `THEME` | `GlobalSignal<Theme>`; `state/theme.rs` | App-wide light/dark choice |
| `screen` | `Signal<Screen>`; `LocalGame` | Setup or playing |
| `game_state` | `Signal<GameState>`; `GameCtx` | Atomic game + legal-hand-move cache |
| `ui_state` | `Store<UiState>`; `GameCtx` | Independently changing interaction/camera fields |
| `OnlineState` | `Store`; online room | Room code, peer ID, phase/snapshot, last error |
| `RoomLink` | Plain `Copy` data; `GameDriver::Online` | Command sender + seat/name memos |
| Downloads | App context; `hooks/downloads.rs` | Release/device/install state; browser builds |
| Desktop updates | App context; `hooks/desktop_updates.rs` | Update status + install commands; survives navigation |

`hooks/game_ctx.rs::use_game_ctx` creates/provides `GameCtx` and installs the
local AI effect. `GameDriver::Local` runs every seat here;
`Online(RoomLink)` sends `PlayMove`/`EndGame` to the authoritative room.
`hooks/online_game.rs` converts snapshots into game state and narrow UI changes;
unchanged game/seat snapshots preserve an open selection.

`GameState` dereferences to core `Game`; read its accessors. Mutate through
`GameState` methods so `hand_info` stays synchronized.

## UI fields

| Group | Fields / consumers |
|---|---|
| Turn | `phase`, `message`; HUD, phase hooks, actions |
| Selection | `selected_tile`, `selected_position`, `hovered`; hand/board hooks |
| Tile drag | `tile_drag`, `suppress_tile_click`; `state/tile_drag.rs`, game pointer handlers/overlay, hand tiles |
| Hand view | `peeked`; shown-hand/HUD memos; cleared on turn change |
| Camera | `zoom`, `pan_x`, `pan_y`, `gesture`, `dragged`, `stage`; camera, viewport, open orientation card |
| Animation | `new_tile`, `last_placed_position`; hands, placed tiles |
| Scoring | `highlighted_color`; scoring paths/rows |

`Hovered` names one target: none, hand index, board position, rotation or score
color. `TurnPhase`: `SelectTile → SelectPosition → SelectRotation`, plus
`WaitingForAi` and `GameOver`; unambiguous choices can auto-play.

## Derived data

| Hook | Value |
|---|---|
| `use_placed_moves` | Placed tiles |
| `use_current_player`, `use_is_game_over`, `use_has_forced_move` | Turn/status facts |
| `use_valid_positions` | Legal empty slots; depends on hovered/selected hand tile |
| `use_preview_placement` | Ghost placement; depends on hover/selection |
| `use_orientations` | Rotation choices; independent of hover |
| `use_highlight_map` | Scoring colors per position |
| `use_board_camera` | Camera commands; subscribes to nothing itself |

`Board` creates each phase memo once and passes `ReadSignal` props to layers.
Consumers memoize the camera fields they display. Shared derivations belong at
the common ancestor; single-consumer derivations stay local.

## Rules

- Atomic mutations → `Signal<T>`; independent fields + narrow readers → `Store<T>`.
- Read inline RSX or inside `use_memo`; use `ui.phase()()`, never whole-store reads.
- Derive with memos, never manually synchronized shadow signals.
- Write per field, using `set_if_changed` when unchanged writes would wake readers.
  Multi-field UI changes use the `#[store]` methods in `state/ui_state.rs`.
- Never `ui.write()`; whole-store `set(UiState::default())` is for full resets.
- No state writes during rendering. Release read guards before writes/awaits.
- No mutable signal props; send callbacks upward.

## Turn flow

`state/actions.rs` owns start/select/place/rotate/end/advance actions, all taking
`GameCtx`. Local moves update `GameState`, call `after_move` (pan reset + animation
markers), frame the board and advance. Online moves send a command; snapshots
update the board. `advance_turn` skips stuck players and sets the next phase.

When local phase becomes `WaitingForAi`, the page-owned effect spawns
`state/ai_service.rs::run_ai_loop`, then advances again. Keeping the task in the
page scope survives tile unmounts; navigation cancels it. Tile drags are cancelled
when the hand, controlled seat or turn phase invalidates them.

Tests: pure decisions beside state; reactive actions/hooks on
`util/test_page.rs::GamePage`; render boundaries in `pages/game/rerender_tests.rs`.
