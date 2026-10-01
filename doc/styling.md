# CSS & Styling

All static rules live in CSS. Rust-side `style:` attributes only set
**CSS custom properties** — no raw `background:` / `color:` / `transform:`
strings are built in Rust.

## Files

| File | Contains |
|---|---|
| `main.css` | Reset, theme tokens, shared primitives (`.hex-frame`, `.btn-*`) |
| `board.css` | Board container, layers, tiles, empty slots, rotation bar, results overlay |
| `hand.css` | Player-hand panel, hand-tile states, outline ring |
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
| `StatusBar` (banner) | `--turn-color` | `.turn-banner { background: var(--turn-color) }` |

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
by `BoardTile`, `HandTile`, `RotationWheel` via `HexSvg`.

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

- `.game-layout` + `.players-2` / `.players-3` / `.players-4` pick the
  grid template for N players.
- `.game-over-layout` overrides rows/columns for the end screen.
- `.board-slot` marks `grid-area: board` for the board container.
- `.player-hand.area-left` / `area-right` / `area-top` / `area-bottom`
  place each hand in its grid cell.

All grid templates live in CSS — Rust only picks which class to apply.

## Responsive

Only two breakpoints: `max-width: 600px` (hide floating hexes, smaller
title) and `max-height: 500px` (compress title vertical gap). Set in
`menu.css`.

## Animations

`animations.css` owns every `@keyframes`. Components trigger them by
adding classes:

| Class | Animation |
|---|---|
| `.hex-tile.just-placed` | scale-up placement |
| `.hand-tile.new-tile` | draw-in fade |
| `.turn-banner.forced` | pulsing brightness |
| `.hand-hex-ring.forced` | pulsing stroke |
| `.floating-hex.hex-*` | entry + floatBob loops |
