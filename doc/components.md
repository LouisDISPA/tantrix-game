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
└── AppLayout                        theme toggle (hidden on the game screen, which has its own)
    └── Router
        ├── MainMenu                 title screen, nav to LocalGame
        ├── LocalGame                use_game_ctx(Local) provides GameCtx
        │   ├── PlayerSetup          player count + AI/Human toggle, difficulty chips → on_start
        │   ├── GamePlay             (shared, below)
        │   └── GameOver             (shared) Play Again / Main Menu
        └── OnlineRoom → OnlineGameView   use_online_game_ctx provides GameCtx (Online driver)
            ├── GamePlay + OnlineSeatTakeover   OnlineHudBadge goes in GamePlay's HUD slot
            └── GameOver             Online Menu / Main Menu, OnlineHudBadge as child (HUD slot)

GamePlay                             one tree for every screen size (game.css lays it out)
├── GameHud                          turn pill, hint, bag, back (✕), online badge slot; on a desktop the top bar,
│   │                                with the hint over the rack
│   └── HudMenu                      ⋯ popover (inline on a wide window)
│       ├── ZoomControls             zoom ± / % / Fit
│       ├── EndGameButton            (hidden on game over, and from watchers) → ConfirmDialog
│       └── ThemeToggle
├── Board                            memos the phase view once, one per layer
│   ├── BoardViewport                pointer/wheel camera host (children slot)
│   │   ├── BoardTilesLayer          placed moves
│   │   ├── BoardEmptyLayer          click targets (EmptyHex)
│   │   └── BoardPreview             ghost of the placement considered
│   └── OrientationPicker            the card of options at the spot, only in SelectRotation
└── hand panel                       PlayerHand × N: a tab (header) + the tiles; one is `shown`
                                     (a phone: tabs over its tiles; a desktop: it is the rack, the others cards
                                     in a rail; a click on a card shows it)
    └── HandTile × hand_size
GameOver                             GameHud + Board (reused) + ScoresPanel (in the panel slot)
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
    game_play.rs       GamePlay: HUD + board + hand panel (the shell game.css lays out)
    game_hud.rs        GameHud: turn pill, hint, bag, back, menu
    player_hand.rs     PlayerHand: one seat's tab + tiles
    shown_hand.rs      whose tiles the panel shows (a pure function)
    game_over.rs       GameOver: final board + scores + exit buttons
  local_game/          LocalGame (Setup → Playing → GameOver), player_setup.rs
  online/game.rs       OnlineGameView: the game screens fed by room snapshots
components/
  board/               Board submodule — the layers and the card of options
    mod.rs             Board composer (measures the stage; no logic)
    viewport.rs        pointer/wheel camera host
    tiles_layer.rs     placed tiles
    empty_layer.rs     empty hex slots (BoardEmptyLayer) + preview ghost (BoardPreview)
    controls.rs        ZoomControls (zoom, Fit), EndGameButton
    orientation_picker.rs  the card of options at the picked spot
  tile/                Shared tile primitives
    hex_svg.rs         pure SVG primitive (polygon + color curves)
    board_tile.rs      BoardTile (Placed | Preview variant)
    empty_hex.rs       empty hex slot + click/hover dispatch
    hand_tile.rs       interactive tile in a player's hand
  hud_menu.rs          HudMenu: the ⋯ popover (owns its open flag)
  theme_toggle.rs      ThemeToggle
  scores_panel.rs      end-of-game scores card
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
| `ZoomControls` | — |
| `EndGameButton` | `on_click: EventHandler<()>` |
| `OrientationPicker` | `orientations: ReadSignal<Option<Orientations>>`, `placed: ReadSignal<Vec<Move>>` |
| `HudMenu`, `GameHud`, `GamePlay` | `children: Element` (menu items / the page's HUD badge) |
| `ThemeToggle` | `class: String`, `labelled: bool` |
| `BoardTile` | `placement: Placement`, `hex_size: f64`, `variant: BoardTileVariant`, `highlight_colors: Vec<Color>`, `just_placed: bool`, `dimmed: bool` |
| `EmptyHex` | `position: Position`, `hex_size: f64`, `forced: bool`, `faint: bool`, `picked: bool` |
| `HandTile` | `color: Color`, `index: usize` |
| `HexSvg` | `tile: Tile`, `rotation: Rotation`, `cx: f64`, `cy: f64`, `hex_size: f64`, `base_class`, `highlight_colors` |
| `PlayerHand` | `color: Color`, `shown: ReadSignal<Color>` |
| `ScoresPanel` | `children: Element` (action buttons) |
| `GameOver` | `again_label: String`, `on_again: EventHandler<()>`, `children: Element` (page overlays) |
| `PlayerSetup` | `seed: Option<u64>`, `on_start: EventHandler<GameConfig>` |
| `DifficultyPicker` | `level: Difficulty`, `on_change: EventHandler<Difficulty>` |
| `ScoreRow` | `color: Color`, `score: usize`, `is_winner: bool` |

Plain `T` by default. `ReadSignal<T>` where a parent derives a value once
for a child that should re-render on its own when it changes: `Board`
hands each layer its memo (Pattern 3 of the `dioxus-hoisting` skill), and
`HandTile`'s `index` follows its tile as the hand shrinks.

## Context-readers

| Component | Reads | Via |
|---|---|---|
| `Board` | is-game-over; creates the placed-moves and phase-view memos for its layers | `use_is_game_over`, `use_placed_moves`, `use_valid_positions`, `use_preview_placement`, `use_orientations`, `use_current_player` (the turn's colour) |
| `GameHud` | phase hints, message, bag count, current player, can go back | memos (`use_current_player`, `use_has_forced_move`) + `message` inline |
| `BoardViewport` | camera transform, dragging, center of the placed moves | memos; `use_board_camera` for the pointer handlers |
| `ZoomControls` | zoom level | memo; `use_board_camera` for the handlers |
| `BoardTilesLayer` | highlights, last placed | `use_highlight_map` + one `last_placed` memo for every tile |
| `BoardEmptyLayer`, `BoardPreview` | their `ReadSignal` prop | props from `Board` |
| `OrientationPicker` | its `ReadSignal` props; the camera and stage only while the card is open (+ `hovered` write on press) | props from `Board`, memo over `UiState` |
| `BoardTile`, `HexSvg` | nothing | props only |
| `EmptyHex` | hovered (write), dispatches `click_position` | direct |
| `HandTile` | hand, hand_info, selected_tile, new_tile, phase, `driver.controls` | memos from `GameCtx` |
| `PlayerHand` | is_current, is_shown, hand, score, `driver.my_color` / `player_name` | one memo each |
| `EndGameButton` | `driver.can_end_game` | memo (`GameHud` confirms, then dispatches `end_game`) |
| `ScoresPanel` | scores, rows, winners | memos (the board is scored once) |
| `ScoreRow` | highlighted_color | memo |
| `GamePlay` | the seats, whose tiles show (`shown_hand`) | memos; an effect ends a peek when the turn changes |
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
`HandTile`, each option of the orientation card) render a hex via `HexSvg`. Wrappers
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
