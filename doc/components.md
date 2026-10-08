# Components

Three rules for this codebase:

1. **Props are identity.** A component takes what it *is*, not what it
   *shows* — the rest comes from context + memos.
2. **Read at the deepest scope that cares.** Don't drill state through
   three parents.
3. **Pure primitives are pure.** Anything named `…Svg`, `…Hex`, `HexSvg`
   takes plain values and reads no context.

## Tree

```
App (lib.rs)
└── AppLayout                        theme toggle
    └── Router
        ├── MainMenu                 title screen, nav to LocalGame
        ├── LocalGame                use_game_ctx(Local) provides GameCtx
        │   ├── PlayerSetup          player count + AI/Human toggle, difficulty chips → on_start
        │   ├── GamePlay             (shared, below)
        │   └── GameOver             (shared) Play Again / Main Menu
        └── OnlineRoom → OnlineGameView   use_online_game_ctx provides GameCtx (Online driver)
            ├── GamePlay + OnlineSeatTakeover + OnlineHudBadge
            └── GameOver             Online Menu / Main Menu, OnlineHudBadge as child

GamePlay                             grid: Board + PlayerHand × N
├── Board                            memos the phase view once, one per layer
│   ├── StatusBar                    phase banner + counters
│   ├── EndGameButton                (hidden on game over, and from watchers)
│   ├── ZoomControls                 +/- zoom buttons
│   ├── BoardViewport                pan/zoom host (children slot)
│   │   ├── BoardTilesLayer          placed moves
│   │   ├── BoardEmptyLayer          click targets
│   │   └── BoardPreview             ghost of the placement considered
│   └── RotationWheel                only renders in SelectRotation phase
└── PlayerHand × N                   one per active color
    └── HandTile × hand_size
GameOver                             Board (reused) + ScoresPanel + exit buttons
```

`GamePlay` and `GameOver` live in `pages/game/` and serve both pages. They
read the `GameCtx` their page provides; its `driver` (`state/game_driver.rs`)
says who plays — `Local`: this device plays every seat, AI seats included;
`Online(RoomLink)`: moves go to the room. Components ask the driver
(`controls`, `can_end_game`, `my_color`, `player_name`) and call turn
actions (`execute_move`, `end_game`) that dispatch on it, so nothing under
`components/` knows about the session.

## Directories

```
pages/
  game/                game screens shared by local + online play
    game_play.rs       GamePlay: board + hands grid
    game_over.rs       GameOver: final board + scores + exit buttons
  local_game/          LocalGame (Setup → Playing → GameOver), player_setup.rs
  online/game.rs       OnlineGameView: the game screens fed by room snapshots
components/
  board/               Board submodule — 5 layers + the rotation wheel
    mod.rs             Board composer (5 children, no logic)
    viewport.rs        zoom/pan host
    tiles_layer.rs     placed tiles
    empty_layer.rs     empty hex slots (BoardEmptyLayer) + preview ghost (BoardPreview)
    controls.rs        zoom buttons, end-game button
    rotation_wheel.rs  conditional rotation picker
  tile/                Shared tile primitives
    hex_svg.rs         pure SVG primitive (polygon + color curves)
    board_tile.rs      BoardTile (Placed | Preview variant)
    empty_hex.rs       empty hex slot + click/hover dispatch
    hand_tile.rs       interactive tile in a player's hand
  player_hand.rs       one player's hand panel
  scores_panel.rs      end-of-game scores card
  status_bar.rs        phase banner + counters
  title_decoration.rs  floating-hex decoration for title screens
```

## Props by component

Only what's *external identity* is a prop. Everything else is context or
a memo.

| Component | Props |
|---|---|
| `Board` | — |
| `BoardViewport` | `placed_moves: ReadSignal<Vec<Move>>`, `children: Element` |
| `BoardTilesLayer` | `placed_moves: ReadSignal<Vec<Move>>`, `dimmed: bool` |
| `BoardEmptyLayer` | `positions: ReadSignal<Vec<ValidPosition>>` |
| `BoardPreview` | `placement: ReadSignal<Option<Placement>>` |
| `ZoomControls`, `EndGameButton` | — |
| `RotationWheel` | `wheel: ReadSignal<Option<WheelData>>` |
| `BoardTile` | `placement: Placement`, `hex_size: f64`, `variant: BoardTileVariant`, `highlight_colors: Vec<Color>`, `just_placed: bool`, `dimmed: bool` |
| `EmptyHex` | `position: Position`, `hex_size: f64`, `forced: bool`, `faint: bool` |
| `HandTile` | `color: Color`, `index: usize` |
| `HexSvg` | `tile: Tile`, `rotation: Rotation`, `cx: f64`, `cy: f64`, `hex_size: f64`, `base_class`, `highlight_colors` |
| `PlayerHand` | `color: Color`, `position: HandPosition` |
| `ScoresPanel` | `children: Element` (action buttons) |
| `GameOver` | `again_label: String`, `on_again: EventHandler<()>`, `children: Element` (page overlays) |
| `PlayerSetup` | `seed: Option<u64>`, `on_start: EventHandler<GameConfig>` |
| `DifficultyPicker` | `level: Difficulty`, `on_change: EventHandler<Difficulty>` |
| `ScoreRow` | `color: Color`, `score: usize`, `is_winner: bool` |
| `StatusBar` | — |

Plain `T` by default. `ReadSignal<T>` where a parent derives a value once
for a child that should re-render on its own when it changes: `Board`
hands each layer its memo (Pattern 3 of the `dioxus-hoisting` skill), and
`HandTile`'s `index` follows its tile as the hand shrinks.

## Context-readers

| Component | Reads | Via |
|---|---|---|
| `Board` | is-game-over; creates the placed-moves and phase-view memos for its layers | `use_is_game_over`, `use_placed_moves`, `use_valid_positions`, `use_preview_placement`, `use_rotation_wheel` |
| `StatusBar` | phase text, message, bag count, current player | memos (`use_current_player`, `use_has_forced_move`) + `message` inline |
| `BoardViewport` | camera transform, dragging, center of the placed moves | memos; `use_board_camera` for the handlers |
| `ZoomControls` | zoom level | memo; `use_board_camera` for the handlers |
| `BoardTilesLayer` | highlights, last placed | `use_highlight_map` + one `last_placed` memo for every tile |
| `BoardEmptyLayer`, `BoardPreview`, `RotationWheel` | their `ReadSignal` prop (+ `hovered` write for the wheel) | props from `Board` |
| `BoardTile`, `HexSvg` | nothing | props only |
| `EmptyHex` | hovered (write), dispatches `click_position` | direct |
| `HandTile` | hand, hand_info, selected_tile, new_tile, phase, `driver.controls` | memos from `GameCtx` |
| `PlayerHand` | is_current, hand, `driver.my_color` / `player_name` | one memo each |
| `EndGameButton` | `driver.can_end_game`, dispatches `end_game` | memo |
| `ScoresPanel` | scores, rows, winners | memos (the board is scored once) |
| `ScoreRow` | highlighted_color | memo |
| `GamePlay` | where each hand sits | memo |
| `LocalGame`, `OnlineGameView`, `OnlineRoomSession` | which screen to show | memos over the phase (game over? started? which room screen?) |

## Event dispatch

Components wire `onclick` / `onmouseenter` / `onmouseleave` directly to
context writes or actions — no `EventHandler<T>` props threaded through
parents.

```rust
// EmptyHex onclick — no prop, just context
onclick: move |_| click_position(ctx, position),
onmouseenter: move |_| hovered.set(Hovered::BoardPosition(position)),
```

## Re-renders, pinned by tests

A component re-renders when a signal it read while rendering is written,
or when its parent passes it different props. So components read memos
(which only notify on a changed value) or narrow store fields, and every
multi-field `UiState` change writes field by field (see `signals.md`).
What that buys is pinned in `pages/game/rerender_tests.rs` and
`pages/online/game.rs`'s tests: they mount the real game screen in a
`VirtualDom`, make one change, and count renders with
`util::render_count` — a test-only counter components call behind
`#[cfg(test)]`. A hover re-renders only `BoardPreview`; a drag only
`BoardViewport`; a zoom only it and `ZoomControls`; a move only the hands
and tiles it changes; a room snapshot never re-renders `OnlineGameView`.
To pin a new component, add the `record` call to its body and assert on
its count.

What components decide is tested without rendering them: `HandTile`'s
state is `compute_state(&GameState, selected, color, index)`, a plain
function its memo calls (reading the selection only for the player to
move), and the board layers' memos are read on `util::test_page::GamePage`
— a page with a `GameCtx` and no screens — after the same changes a hover
or click makes.

## HexSvg — the one primitive

All four tile-shaped components (`BoardTile`, `EmptyHex`'s polygon,
`HandTile`, each rotation-wheel slot) render a hex via `HexSvg`. Wrappers
own positioning + state classes; `HexSvg` owns geometry.

## When in doubt

- Need a new in-game screen for local and online play? → `pages/game/`
- Need a new local-only screen? → `pages/local_game/`
- Need a new reusable board layer? → `components/board/`
- Need a new tile-shaped thing? → reuse `HexSvg` inside a new wrapper
- Need a new piece of derived state used by >1 component? → `hooks/`
- Derived state used by just one component? → inline `use_memo`

See `dioxus-components` / `dioxus-state` / `dioxus-styling` skills for
patterns.
