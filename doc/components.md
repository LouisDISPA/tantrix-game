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
App (main.rs)
└── AppLayout                        theme toggle
    └── Router
        ├── MainMenu                 title screen, nav to LocalGame
        └── LocalGame                provides GameCtx
            ├── PlayerSetup          player count + AI/Human toggle → Screen::Playing
            ├── GamePlay             grid: Board + PlayerHand × N
            │   ├── Board
            │   │   ├── StatusBar           phase banner + counters
            │   │   ├── EndGameButton       (hidden on game over)
            │   │   ├── ZoomControls        +/- zoom buttons
            │   │   ├── BoardViewport       pan/zoom host (children slot)
            │   │   │   ├── BoardTilesLayer placed moves
            │   │   │   └── BoardEmptyLayer click targets + preview
            │   │   └── RotationWheel       only renders in SelectRotation phase
            │   └── PlayerHand × N   one per active color
            │       └── HandTile × hand_size
            └── GameOver             Board (reused) + ScoresPanel
```

## Directories

```
components/
  board/               Board submodule — 5 layers + the rotation wheel
    mod.rs             Board composer (5 children, no logic)
    viewport.rs        zoom/pan host
    tiles_layer.rs     placed tiles
    empty_layer.rs     empty hex slots + preview ghost
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
| `BoardViewport` | `children: Element` |
| `BoardTilesLayer` | — |
| `BoardEmptyLayer` | — |
| `ZoomControls`, `EndGameButton` | — |
| `RotationWheel` | — (reads `use_phase_view` itself) |
| `BoardTile` | `placement: Placement`, `hex_size: f64`, `variant: BoardTileVariant` |
| `EmptyHex` | `position: Position`, `hex_size: f64`, `forced: bool`, `faint: bool` |
| `HandTile` | `color: Color`, `index: usize` |
| `HexSvg` | `tile: Tile`, `rotation: Rotation`, `cx: f64`, `cy: f64`, `hex_size: f64`, `base_class`, `highlight_colors` |
| `PlayerHand` | `color: Color`, `position: HandPosition` |
| `ScoresPanel` | `children: Element` (action buttons) |
| `ScoreRow` | `color: Color` |
| `StatusBar` | — |

Plain `T` throughout; no `ReadSignal<T>` props.

## Context-readers

| Component | Reads | Via |
|---|---|---|
| `StatusBar` | phase, message, bag/board counts, has_forced | direct + `use_current_player`, `use_has_forced_move` |
| `BoardViewport` | camera fields, placed moves | `use_board_camera`, `use_placed_moves` |
| `BoardTilesLayer` | placed moves, highlights, is-game-over, last placed | `use_placed_moves`, `use_highlight_map`, `use_is_game_over` |
| `BoardEmptyLayer` | phase_view (positions + preview) | `use_phase_view` |
| `BoardTile` | highlight_map, is-game-over, last placed | memos per-tile |
| `EmptyHex` | hovered (write), dispatches `click_position` | direct |
| `RotationWheel` | phase_view.wheel, hovered (write) | `use_phase_view` |
| `HandTile` | hand, hand_info, selected_tile, new_tile, phase | memos from `GameCtx` |
| `PlayerHand` | is_current, hand length | `game_state.read()` |
| `ScoresPanel` | active players, scores, winners | `game_state.read()` |
| `ScoreRow` | score, winner flag, highlighted_color | memos + `ui_state.highlighted_color` |

## Event dispatch

Components wire `onclick` / `onmouseenter` / `onmouseleave` directly to
context writes or actions — no `EventHandler<T>` props threaded through
parents.

```rust
// EmptyHex onclick — no prop, just context
onclick: move |_| click_position(ctx, position),
onmouseenter: move |_| hovered.set(Hovered::BoardPosition(position)),
```

## HexSvg — the one primitive

All four tile-shaped components (`BoardTile`, `EmptyHex`'s polygon,
`HandTile`, each rotation-wheel slot) render a hex via `HexSvg`. Wrappers
own positioning + state classes; `HexSvg` owns geometry.

## When in doubt

- Need a new screen? → `pages/local_game/screens/`
- Need a new reusable board layer? → `components/board/`
- Need a new tile-shaped thing? → reuse `HexSvg` inside a new wrapper
- Need a new piece of derived state used by >1 component? → `hooks/`
- Derived state used by just one component? → inline `use_memo`

See `dioxus-components` / `dioxus-state` / `dioxus-styling` skills for
patterns.
