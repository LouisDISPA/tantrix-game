# Game implementation

`tantrix-game` owns the rules; UI and server adapters depend on it.
See the [workspace map](../README.md#workspace), [AI and tournaments](ai.md),
[multiplayer](MULTIPLAYER.md), and [Worker deployment](../cloudflare/README.md).

## Core model

All paths below are under `tantrix-game/src/`.

| Type | Definition |
|---|---|
| `Tile`, `Color` (`tiles.rs`) | 56 tiles, each with 3 colored edge pairs; Red / Blue / Yellow / Green |
| `Position`, `Direction`, `Rotation` (`coordinate_system.rs`) | Axial `(i8, i8)`, six edges, rotations in 60° steps |
| `Placement` (`moves.rs`) | Tile + position + rotation: player input |
| `Move` | Placement + `forced`, determined by the rules |
| `Board` (`board.rs`) | `ArrayVec<Move, 56>`, sorted by position |
| `Hand`, `Bag` | Up to six tiles per player; shared draw pile |
| `Game` (`game.rs`) | Source of truth: players, board, hands, bag, turn, history, cached legal moves |
| `TurnEvent` (`history.rs`) | Played move, skipped turn or retirement |

## Rules and turns

`rules.rs::Board::possible_moves` enforces:

- **Golden rule:** touching edges match colors.
- **Forced priority:** fill a space with 3+ neighbors whenever possible.
- **While the bag has tiles:** no same-color triples, fourth neighbor around
  a forced space, or controlled-side plays.

A turn fills forced spaces, plays one free move, then fills forced spaces
again (`TurnStep::BeforeFreeMove` / `AfterFreeMove`). Forced priority still
applies after the bag empties; only the three restrictions lift.

`Game::play_move(Placement)` checks the held tile and legal moves, derives
`forced`, draws replacements and advances the turn when appropriate. It
returns `PlayedMove`, or `MoveError::TileNotInHand` / `InvalidMove`.
`Board::insert_move` only checks occupancy; it is not a legal-play API.

`Game::skip_stuck_players` skips each player at most once and reports a
stall when nobody can move. `Game::advance` also plays AI turns until a
human can move, the game ends, or it stalls. The app, tournament and room
engine use the same skip routine.

Natural completion means an empty bag and empty hands. Callers also handle
stalls and explicit endings.

## Scoring

A line scores one point per tile; a loop scores two. Each color keeps its
best line or loop. `Game::winners` returns all active colors tied for first;
`winners_from` accepts scores already computed.

- `Board::scores` / `scoring_paths`: walk every path once.
- `Board::lines`: line/loop lengths and open-end positions for AI evaluation.
- `Board::scores_after`: scores after a legal move without a board copy,
  except when closing a loop.
- `Board::score_after`: one color after a move, exact without copying.
- `Board::indexed`: reuse a dense position grid across candidate moves.

Legal-move generation also builds one `Grid` per pass and matches rotations
with packed `Edges` (two bits per direction). Reference move-by-move rules
remain tested against the optimized pass, including move order.

## History, seeds and persistence

| Event | Recorded data |
|---|---|
| `Played` | Previous player/turn step, legal `Move`, hand index, drawn tiles in order |
| `Skipped` | Previous player/turn step |
| `Retired` | Removed color |

`Game::undo` reverts one event, including bag, hand and turn state. It
returns `None` for empty history or a final retirement, whose hand was lost.
`replay_event` validates an event before applying it; failure leaves the
game unchanged. `Game::replay(seed, players, events)` reports the failing
event through `ReplayError`.

`new` / `from_players` generate and retain a random seed; `new_seeded` /
`from_players_seeded` retain the supplied seed. Deals use
`Xoshiro256PlusPlus::seed_from_u64`; `/game?seed=N` reproduces them.
`from_players_with_rng` retains no seed.

`GameData` persists board, hands, bag, turn, optional seed and history;
older saves default to no seed/history. Online rooms use the OS-seeded
thread RNG and retain no seed. Their bag and history recover the deal.
Snapshots expose neither bag contents, seed nor history.

## Frontend flow

Paths here are under `tantrix-app/src/`.

`state/game_state.rs::GameState` wraps `Game` and caches
`hand_info = moves_by_tile()`. Mutations refresh this cache; `Game` itself
lazily caches legal moves. Components read through `GameState`'s deref.

`GameCtx` holds game state, UI state and `GameDriver`: `Local` controls all
seats and runs AI turns; `Online(RoomLink)` sends only this seat's intents
and receives authoritative snapshots. Shared game screens live in
`pages/game/`; `pages/local_game/` handles setup and replay, and
`pages/online/game.rs` selects online play or results.

`state/ui_state.rs::TurnPhase` drives selection:

```text
SelectTile → SelectPosition → SelectRotation → SelectTile
                     └─ one rotation ──────────┘
AI turn → WaitingForAi             game ends → GameOver
```

`state/actions.rs` owns `start_game`, `click_tile`, `click_position`,
`pick_rotation`, `execute_move`, `advance_turn` and `end_game`.
A tile with one position and one rotation plays immediately; the empty
board's opening tile uses the first rotation. Hover previews the unique
move except for the opening tile. Driver checks limit clicks to a controlled
seat on its turn.

Local `execute_move` updates `GameState`, calls `UiState::after_move`, frames
the board and advances the turn. Online it sends `PlayMove`; the snapshot
updates the board. `advance_turn` skips stuck seats and selects AI, human,
or game-over state.

`hooks/game_ctx.rs` starts the page-scoped AI loop from `WaitingForAi`.
`state/ai_service.rs` waits 500 ms before synchronous search, shows the
chosen tile (500 ms) and destination (400 ms), commits, updates UI/camera,
then waits 300 ms for the draw animation. Unmounting cancels the task. Rendering never starts a turn.

Results show the board with scoring overlays and `ScoresPanel`; hovered or
locked colors filter `hooks/scoring.rs::use_highlight_map`. Exits are Play
Again (local), Online Menu (online), and Main Menu.

## AI service

`player.rs::TantrixAi` requires `name`, `clone_box`, and
`choose_move(&AiView) -> Move`; `with_seed` defaults to cloning.
`Game::ai_view` exposes board, face-up hands, player order, turn step, bag
count and nonempty legal moves. `AiView::unseen_tiles` excludes known tiles;
`determinize(rng)` shuffles those into a private simulated bag. An AI must
return a supplied legal move; callers play its `placement()`.

App difficulties: Easy = Smart, Medium = Strategist (default), Hard =
Lookahead, Expert = MonteCarlo. The terminal game uses Random.
