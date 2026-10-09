# CSS & Styling

All static rules live in CSS. Rust-side `style:` attributes only set
**CSS custom properties** — no raw `background:` / `color:` / `transform:`
strings are built in Rust.

## Files

| File | Contains |
|---|---|
| `main.css` | Reset, theme tokens, shared primitives (`.hex-frame`, `.btn-*`) |
| `board.css` | Board container, layers, tiles, empty slots, the card of orientation options, results card |
| `hand.css` | Player-hand header (a tab, or a card), hand-tile states, outline ring |
| `game.css` | The game screen's shell: its backdrop and glass, HUD row, board, hand panel as a floating dock (*stack*) or a side column (*side*), the HUD pill, the menu |
| `desk.css` | The game screen on a desktop window (landscape, ≥ 900 × 640): floating top bar, a rail of the other players on the left, a rack under the board |
| `menu.css` | Title screen, setup card, floating hex decoration |
| `animations.css` | All `@keyframes` + classes that apply them |

## Theme tokens

`:root` defines theme-independent tokens. `[data-theme="light"]` /
`[data-theme="dark"]` override the palette. Toggle via `THEME` global
signal which flips the `data-theme` attribute on the root.

### Geometry / clip-paths

```css
--clip-hex          /* pointy-top hex for frames & swatches   */
--clip-btn          /* wide hex-cut primary button            */
--clip-btn-tight    /* same, narrower cut (results panel)     */
```

Every `clip-path` reads from these — no literal `polygon(…)` values in
rules.

### Colors

```css
--color-red / --color-blue / --color-yellow / --color-green   /* OKLCH */
--glow-red / --glow-blue / ...    /* color-mix(color × --glow-alpha)  */
--tile-fill / --tile-stroke       /* board tile body                   */
--hex-empty                       /* empty-slot fill                   */
--hex-frame-opacity               /* 0.06 light / 0.10 dark            */
```

Glow filters and scoring-path highlights use `--glow-*`, never hard-coded
`rgba(230, 57, 70, 0.25)`.

### Per-player color

Components that render something in a player's color set `--player-color`
on a wrapper element. Descendants read it: `background: var(--player-color)`,
`color: var(--player-color)`, etc. One assignment, many uses.

## Dynamic values → CSS custom properties

Inline `style:` in Rust only assigns custom properties. The actual
property (transform / left / top / …) is declared in CSS.

| Component | Sets | CSS consumes |
|---|---|---|
| `BoardViewport` | `--zoom`, `--pan-x`, `--pan-y` | `.board-viewport { transform: scale(var(--zoom)) translate(…) }` |
| `BoardViewport` (inner) | `--center-x`, `--center-y` | `.board-tiles-layer { transform: translate(…) }` |
| `BoardTile`, `EmptyHex` | `--hx`, `--hy`, `--hw`, `--hh` | `.board-tile { left: var(--hx); top: var(--hy); … }` |
| `PlayerHand`, `ScoreRow` | `--player-color` | class-based usage |
| `Board` (its `.board-container`) | `--turn-color`, via a `.color-*` class | the legal spots, the card of options |
| `GameHud` | `--p` (how full the bag is) | `.hud-chip-bag::after`, the bag's bar on a desktop |
| `OrientationPicker` | `--card-w`, `--card-h`, `--arrow-x`, `--arrow-y` | `.orientation-card` (its place on the stage, `left` / `top`, is worked out in Rust) |

Transitions, easing, and animation live entirely in CSS — changing a
curve doesn't touch Rust.

## Shared primitives

### `.hex-frame`

One base class for the four animated hex-gradient frames behind cards
(title, setup, results). Callers add a size-only modifier:

```rust
rsx! { div { class: "hex-frame results-hex-frame" } }
```

```css
.hex-frame { clip-path: var(--clip-hex); background: linear-gradient(…); animation: hexFramePulse …; }
.results-hex-frame { width: 340px; height: 340px; }
.title-hex-frame   { width: 320px; height: 320px; }
.setup-hex-frame   { width: 380px; height: 380px; }
```

### `.hex-tile-fill` / `.hex-curve`

Shared fill & stroke rules for every tile polygon and color curve, used
by `BoardTile`, `HandTile` and the card of options via `HexSvg`.

## Class composition in Rust

Use Dioxus's multi-`class:` attribute — no string concatenation.

```rust
div {
    class: "board-tile hex-tile",
    class: if is_preview { "tile-preview" },
    class: if just_placed() { "just-placed" },
    class: if is_game_over() { "dimmed-tile" },
    style: "{style}",   // only --hx/--hy/--hw/--hh
}
```

## Layout classes

One component tree serves every screen. The stylesheets pick the layout and
Rust never branches on the window size. `.game-layout` is the shell grid: the
HUD (`.game-hud`), the board (`.board-slot`) and the panel (`.hand-panel`, or
`.results-panel` at game over).

**The board is the screen.** `.game-layout` paints the backdrop (the hex
grid, `--board-pattern`, lit from the middle) and clips at the window with
`overflow: clip`, so focusing or scrolling to a hex out there never slides it.
The board's cell (`.board-stage`) does not clip: the HUD and the panel are
**glass cards** (`--glass`, `--glass-line`, a blur) floating over it, and a
zoom or a pan runs the board under them. The camera frames the board in the
cell the cards leave, so a legal spot is never left under one.

| Layout | When | Panel |
|---|---|---|
| *stack* (`game.css`) | default | a row of tabs (one per seat) over the shown seat's tiles, under the board |
| *side* (`game.css`) | landscape, ≥ 640px wide | the same, as a column beside the board |
| *desk* (`desk.css`) | landscape, ≥ 900 × 640 | the shown seat is a **rack** under the board's cell, the HUD's hint in large type on its header line; every other seat is a **card** (score, readable hand) in a rail on the left; the HUD is a bar of capsules (turn, tools, room) |

- Every `.player-hand` is always in the DOM. In *stack* and *side* they are
  `display: contents`, so each seat's `.hand-header` (a tab) falls into the
  first grid row and the `.shown` seat's `.hand-tiles` span the second.
- A click anywhere on another seat's `.player-hand` shows its tiles (it
  becomes `.shown`). On a phone only its header tab is visible to click.
- `.players-2` / `.players-3` / `.players-4` set `--seats` (the number of
  tabs) and, in *desk*, the shape of the cards.
- *desk* sizes everything in `em` of `--fs` (`clamp(13px, min(1.05vw, 1.9vh),
  26px)`), so the interface scales with the window; the board follows
  through the camera's home zoom (`svg/camera_math.rs`).
- *desk* takes the rack (the shown `.player-hand`) and the hint
  (`.hud-chip-phase`) out of flow with `position: absolute` in the `rack`
  grid area. Their containing block is `.game-layout`, so nothing between
  them and it may be positioned (no `position`, `transform`, `filter`,
  `backdrop-filter` or `contain` on `.game-hud`, `.hud-chip`, `.hand-panel`:
  the rail has no glass of its own, and the turn chip, which holds the hint,
  a translucent fill without the blur).
- `.hud-menu` is a popover with a `⋯` toggle; from `min-width: 900px` its
  items sit in the HUD row instead.
- `--turn-color` (from the `.color-*` class on `.board-container`) colours
  the legal spots and the card of options; the player colours otherwise
  appear only as small swatches, the underline of the shown tab and the
  turn dot.
- Touch vs mouse is `@media (hover: hover)` / `(pointer: coarse)`, never
  user-agent sniffing: hover effects are mouse-only, a touch screen keeps
  only Fit of the zoom buttons.

All grid templates live in CSS — Rust only picks which classes to apply.

## Responsive

Menu screens: `max-width: 600px` (hide floating hexes, smaller title) and
`max-height: 500px` (compress title vertical gap), in `menu.css`. The game
screen (`game.css`): `side` at landscape ≥ 640px wide, the HUD pill's short
hint under 480px, its tighter pill under 400px, the inline menu from 900px.
`desk.css` takes over at landscape ≥ 900 × 640; there the room's connection
label gives way to its code under 1100px. Heights use `100dvh` (with a
`100vh` fallback), and the HUD and panel pad by `env(safe-area-inset-*)`.

## Animations

`animations.css` owns every `@keyframes`. Components trigger them by
adding classes:

| Class | Animation |
|---|---|
| `.hex-tile.just-placed` | scale-up placement |
| `.hand-tile.new-tile` | draw-in fade |
| `.game-layout` | fade-in |
| `.hand-hex-ring.forced` | pulsing stroke |
| `.floating-hex.hex-*` | entry + floatBob loops |
