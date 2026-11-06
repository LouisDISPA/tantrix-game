# Components

Paths below are relative to `tantrix-app/src/`.

## Map

| Location | Responsibility |
|---|---|
| `lib.rs` | App root, CSS, startup overlay, download/update contexts, router |
| `route.rs` | `AppLayout`: menu background, outlet, theme toggle; route definitions |
| `pages/` | Main menu, learn, about, download, not-found and desktop-update screens |
| `pages/local_game/` | Setup → playing/results; creates local `GameCtx` |
| `pages/online/` | Entry/join, session driver, room screens, lobby, seat takeover, HUD badge |
| `pages/game/` | Shared local/online game: HUD, hands, results, turn banner, dragged tile |
| `components/board/` | Board composer, viewport, placed/empty/preview layers, orientation picker, controls |
| `components/tile/` | `HexSvg`, `BoardTile`, `EmptyHex`, `HandTile` |
| `components/` | Shared menu, dialog, input, badge, theme and loading widgets |
| `hooks/` | Game contexts, camera, memoized board/phase/scoring data, downloads, updates |
| `state/` | Game/UI/session state, actions, AI loop, tile dragging, settings |
| `svg/` | Pure geometry, camera arithmetic, card placement, color paths |

## Game tree

```text
LocalGame / OnlineGameView → GameCtx
├── GamePlay
│   ├── GameHud → HudMenu → zoom / end game / theme
│   ├── TurnBanner
│   ├── Board
│   │   ├── BoardViewport → BoardTilesLayer / BoardEmptyLayer / BoardPreview
│   │   └── OrientationPicker
│   ├── PlayerHand × seat → HandTile × tile
│   └── GameDraggedTile
└── GameOver → GameHud / Board / ScoresPanel
```

`GamePlay`/`GameHud` accept a HUD child slot; online play supplies its badge.
`GameDriver::Local` runs turns here; `Online(RoomLink)` sends commands and mirrors
room snapshots. Game widgets use `GameCtx` and actions, never session commands.

## Tiers

Dependencies point down: page → organism → molecule → atom.

| Tier | Location | Examples | May own or read |
|---|---|---|---|
| Page | `pages/{feature}/` | `OnlineRoom`, `LocalGame`, `OnlineLobbyView`, `GamePlay` | Route params, phase gating, the backbone: `Store<OnlineState>`, session sender, `GameCtx` (`use_game_ctx`) |
| Organism | `pages/{feature}/` | `OnlineHudBadge`, `LobbyColorSeat`, `GameHud`, `PlayerHand` | Backbone as a read-only prop (`state: Store<OnlineState>`) or `GameCtx`; memos; small UI flags |
| Molecule | `components/` | `Board`, `HandTile`, `ScoresPanel`, `ShareChip`, `NicknameInput` | `GameCtx` through context or `hooks/`; `state::actions`; behaviour-local state (`ShareChip.copied`, `NicknameInput.draft`) |
| Atom | `components/` | `HexSvg`, `ColorChip`, `RoomCodeBadge` | Nothing: props in, RSX out |

`GameCtx` is shared by local and online play, so `components/` may read it and
dispatch actions through the driver. Session state may not cross into
`components/`: no `crate::pages`, `state::online_state` or `SessionCommand`.
`components/tier_tests.rs` checks this.

Classify a component in order: route screen, or reads session state → page or
organism; reusable, reads only `GameCtx`/hooks or owns behaviour-local state →
molecule; renders props only → atom. Keep state local only when no other
component reads or writes it; otherwise hoist it to the common page.

- Name atoms and molecules with the bare domain noun (`ShareChip`). Prefix
  organisms with their feature when the name would be generic (`LobbyColorSeat`).
- A feature's `mod.rs` or page composes its organisms; organisms never import
  sibling organisms. Remove pass-through organisms that only forward props.
- Name a page file after its component (`room.rs` for `OnlineRoom`). Use
  `{feature}/screens/` only for several leaf screens of one route.
- Keep atoms and molecules flat in `components/` (plus the `board/` and `tile/`
  families); no `ui/`, `views/` or `widgets/` buckets.
- Provide a context only for data read across many components or routes
  (`GameCtx`, downloads/updates, `MenuVisit`, `RoomServer`); pass the rest as props.

## Boundaries

- Pages compose features; reusable widgets live in `components/`. Keep session
  state in `pages/online/` and `state/online_state/`.
- Props identify a widget (`HandTile`: color + index). Read shared game data from
  `GameCtx`; derive display values with memos. Pure SVG primitives take plain props.
- Plain `T` is the default; `ReadSignal<T>` gives a child its own subscription;
  `EventHandler<T>` sends events upward. No mutable signal props.
- `Board` computes placed moves and each phase memo once, passing them to the
  relevant layer. `PlayerHand` receives `shown: ReadSignal<Color>`;
  `HandTile` receives `index: ReadSignal<usize>` so shrinking hands retain tile identity.
- Key each dynamic list's direct child. Keep reads inside memos or inline RSX;
  avoid subscriptions in the component body.
- Reuse `HexSvg` for tile geometry; wrappers own position and state classes.
- One component tree serves every window size; CSS selects the layout.

## Verification

Pure decisions have adjacent deterministic tests (`compute_state`, `shown_hand`).
`util/test_page.rs` mounts a `GameCtx` without screens; `GamePage::act` settles
reactivity, while `with` does not start pending AI effects.
`pages/game/rerender_tests.rs` and online game tests use `util/render_count.rs`
to pin render boundaries for hover, drag, zoom, moves and room snapshots.

See [state](signals.md), [styling](styling.md) and the Dioxus skills.
