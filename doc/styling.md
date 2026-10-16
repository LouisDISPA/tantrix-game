# Styling

Styles live in `tantrix-app/assets/`; `tantrix-app/src/lib.rs` assembles shared CSS.
SSR embeds it; fullstack browser hydration reuses that tag. Desktop injects the
shared CSS plus `desktop.css` through `src/desktop.rs`.

## Files

| File | Scope |
|---|---|
| `main.css` | Cascade-layer order, reset, base, hex frames |
| `tokens.css` | Spacing, radius, typography, motion, palette, surfaces, theme tokens |
| `components.css` | Shared buttons, cards, inputs, badges, dialogs |
| `board.css` | Board, tiles, empty slots, orientation picker, results |
| `hand.css` | Hand headers/tiles and interaction states |
| `game.css` | Game shell, HUD, stack/side layouts, turn banner, dragged tile |
| `desk.css` | Desktop-size game layout: top bar, left rail, bottom rack |
| `menu.css` | Menu, setup, online entry/lobby, download/about screens |
| `learn.css` | Rules/tutorial screen |
| `animations.css` | Shared keyframes/animation classes |
| `loading.css` | Startup overlay and its motion |
| `desktop.css` | Native window chrome and update UI |

Layers: `reset → tokens → base → components → features → responsive → motion`.

## Conventions

- Keep static presentation, transitions and responsive rules in CSS. Rust supplies
  dynamic geometry and state classes; prefer custom properties over inline rules.
  Existing dragged-tile positioning supplies inline `left`/`top`/`width`.
- Reuse tokenized spacing, radii, motion and surfaces (`.surface`, `.surface-blur`).
  `THEME` sets `data-theme`; palettes override tokens for light/dark.
- Player colors use `--color-*`/`--glow-*`; `.color-*` and `--player-color` carry
  identity. `--turn-color` colors legal slots and rotation choices.
- Reuse `--clip-hex`, `.hex-frame`, `.hex-tile-fill`, `.hex-curve`; `HexSvg` owns
  tile geometry. Use multiple Dioxus `class:` attributes for conditional classes.
- Respect `prefers-reduced-motion`; hover effects use `(hover: hover)`, touch
  controls use `(pointer: coarse)`.

| Dynamic value | Producer → CSS |
|---|---|
| Zoom/pan | `BoardViewport`: `--zoom`, `--pan-x`, `--pan-y` |
| Board center | `BoardViewport`: `--center-x`, `--center-y` |
| Tile position/size | Tile wrappers: `--hx`, `--hy`, `--hw`, `--hh` |
| Bag fill | `GameHud`: `--p` |
| Rotation card | `OrientationPicker`: size/arrow properties from `svg/card_placement.rs` |

## Layout

One component tree, CSS breakpoints only. `.game-layout` is the clipped,
full-window backdrop; the HUD, hands and results float over the board as glass
surfaces. The camera frames the usable `.board-stage`; panning can pass under
cards. Camera home zoom follows measured stage size (`svg/camera_math.rs`).

| Layout | Condition | Hands |
|---|---|---|
| Stack | Default | Tabs above the shown hand, below board |
| Side | Landscape, width ≥ 640px | Tabs/hand beside board |
| Desk | Landscape, width ≥ 900px, height ≥ 640px | Shown hand in bottom rack; other seats in left rail |

Every `PlayerHand` remains mounted. `.shown` selects the rack/tab contents;
`.players-2`/`3`/`4` sets seat geometry. Desk sizing uses `--fs` to scale with the
window. Its rack and phase hint are positioned against `.game-layout`: avoid
introducing containing blocks on intervening HUD/panel elements (position,
transform, filter, backdrop-filter or containment).

The HUD menu becomes inline at 900px width. Menus compact for narrow/short
windows; game hints shorten on small widths. Heights use `100dvh` with a `100vh`
fallback; edge padding includes safe-area insets.
