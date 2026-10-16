# AI players and the tournament

`tantrix-ai` implements `TantrixAi`; `tantrix-tournament` compares AIs.
[`AiView`](game.md#ai-service) exposes the board, face-up hands, turn,
bag count and legal moves, never draw order. Simulations randomize the
unseen tiles with `AiView::determinize`.

## AIs

`all_ais()` returns these in order. Defaults are in `tantrix-ai/src/`.
Historical Elo and timings appear in the championship results below.

| AI | Choice |
|---|---|
| `FirstMove` | First legal move |
| `Random` | Random legal move |
| `Greedy` | Largest own score gain |
| `Defensive` | Own score minus best opponent's |
| `LoopHunter` | Loops weighted 3× tiles, lines 1× |
| `Smart` | Own gain ×1000 − leading opponent's gain ×100; −50 beside their best path's ends |
| `Builder` | One-move line evaluation, opponents ignored |
| `Strategist` | One-move line evaluation minus best opponent's |
| `Lookahead` | Best reply to each of Strategist's top 8 moves |
| `MonteCarlo` | Playouts from Strategist's top 16 moves, successive halving |

App difficulties (`tantrix-app/src/state/config.rs`): Easy = Smart,
Medium = Strategist (default), Hard = Lookahead, Expert = MonteCarlo.
Only Random and MonteCarlo use randomness; `with_seed` makes their choices
repeatable. The tournament seeds both AIs per game.

### The line evaluation (`eval.rs`)

A color scores its longest line (1 point a tile) or loop (2 points a tile),
so it is worth its best line, counting what that line can still become:

- its tiles (a loop: twice its tiles);
- per end a remaining tile can still fill (`Board::lines` gives each end's
  empty position, `Board::edges_toward` + `Edges::accepts` whether a tile
  fits there), `open_end` × the share of tiles left to place: by the golden
  rule, any tile placed at a line's end extends it, whoever plays it;
- more per such end on a forced space (`forced_end`): someone must fill it
  soon, extending the line;
- more per end a tile of the owner's hand fits (`ready_end`);
- a line whose two ends lead into one space a tile can close:
  `closable_loop` × the loop's points; two lines whose ends meet in one
  fillable space: `joinable` × their joined length.

A position is worth the player's value less `opponent` × the best
opponent's. `Weights::BALANCED` came out of two tuning rounds (below);
`Builder` plays `Weights::SELFISH` (`opponent = 0`).

### Lookahead

Ranks legal moves by line evaluation, keeps `width` (default 8), and
answers each with the best next move: the same player's if their turn
continues, otherwise the next player's. Known hands are used; unknown
drawn tiles are omitted.

### MonteCarlo

Keeps `width` (default 16) candidates by line evaluation and spends
`width × playouts` simulations (`playouts` defaults to 16). Each candidate
uses the same randomized deals to reduce draw noise. Greedy playouts
maximize each player's score gain (`IndexedBoard::score_after`), breaking
ties randomly. Reward is final margin against the best opponent, plus 5
for a win or minus 5 for a loss.

Successive halving (`halving = true`, default) drops the worse half each
round and spends more on survivors. Flat mode gives each candidate the
same number of playouts.

## The tournament

```bash
cargo run --release -p tantrix-tournament -- --help
cargo run --release -p tantrix-tournament -- --config tantrix-tournament/configs/championship.toml --headless
```

Each pairing plays `games` games (default 1000). Mirrored pairs swap
seats on the same deal (`--no-mirror` disables this). Game seeds are drawn
before execution and seed both the deal and AIs; results are independent
of thread scheduling. Workers interleave pairings round by round.

Standings show Bradley–Terry Elo (mean 1500; one virtual draw per pairing),
score = wins + half draws with a 95% margin, W/L/D, average points/margin,
and choice time under concurrent load. Results include head-to-head and
first-player scores.

- **TUI:** default on a terminal; live standings/trends, pause, stop (`s`),
  next seed.
- **Headless:** `--headless` or redirected stdout; progress on stderr every
  10% and 30 seconds, final plain-text results on stdout.

### Config files

`--config FILE` reads a TOML file (`tantrix-tournament/configs/`): the
settings (`seed`, `games`, `mirror`, `threads`; flags override them) and
the AIs as `[[ai]]` tables, in order, every AI if there are none. Each has
a `kind` (a name `--list` shows) and an optional `name`; Strategist,
Builder, Lookahead and MonteCarlo take `weights = { … }` (any of the
fields above, the rest keep their defaults), Lookahead and MonteCarlo a
`width`, MonteCarlo `playouts` and `halving`. Anything else is refused.
`configs/example.toml` shows every field. Only the tournament parses
configs (`serde` + `toml`); `tantrix-ai` builds its AIs from plain values.

| Config | Question |
|---|---|
| `championship.toml` | Which AI is the best? Every AI, 1000 games per pairing |
| `search.toml` | How wide and how deep should MonteCarlo search, flat? |
| `halving.toml` | Does successive halving beat flat playouts at the same cost? |
| `weights.toml` | Which way should the line evaluation's weights go? Strategist, one weight changed at a time |
| `example.toml` | Every field, commented |

`--ais A,B` and `--skip A,B` keep or drop AIs by name from the config's (or
every) AI.

## What the tournaments found

Historical runs on mirrored deals; these are recorded results, not a
benchmark of the current checkout. Configs live in
`tantrix-tournament/configs/`; some tuning configs were replaced.
± denotes a score's 95% margin.

### Championship: which AI is the best?

`championship.toml`: every AI, seed 2026, 1000 games per pairing, 45,000
games in 12m54s on 10 threads.

| Rank | AI | Elo | Score | ± | Wins | Losses | Draws | Avg pts | Margin | ms/move |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | MonteCarlo | 2076 | 96.7% | 0.4 | 8653 | 256 | 91 | 27.73 | +14.08 | 29.712 |
| 2 | Lookahead | 1686 | 73.0% | 0.9 | 6406 | 2269 | 325 | 21.66 | +4.58 | 0.564 |
| 3 | Strategist | 1593 | 63.3% | 1.0 | 5488 | 3100 | 412 | 20.14 | +2.28 | 0.158 |
| 4 | Builder | 1527 | 55.5% | 1.0 | 4783 | 3792 | 425 | 20.45 | +0.65 | 0.144 |
| 5 | Smart | 1457 | 46.9% | 1.0 | 4012 | 4567 | 421 | 18.18 | -1.40 | 0.012 |
| 6 | Defensive | 1445 | 45.4% | 1.0 | 3883 | 4709 | 408 | 17.93 | -1.61 | 0.011 |
| 7 | Greedy | 1423 | 42.8% | 1.0 | 3630 | 4932 | 438 | 17.89 | -2.17 | 0.003 |
| 8 | LoopHunter | 1421 | 42.4% | 1.0 | 3618 | 4979 | 403 | 18.10 | -2.27 | 0.007 |
| 9 | Random | 1199 | 18.2% | 0.8 | 1463 | 7184 | 353 | 13.34 | -6.97 | 0.000 |
| 10 | FirstMove | 1172 | 15.8% | 0.8 | 1242 | 7390 | 368 | 12.45 | -7.15 | 0.000 |

```
Head to head: the row AI's score against the column AI, in % (a draw counts half)
           MonteCa Lookahe Strateg Builder   Smart Defensi  Greedy LoopHun  Random FirstMo
MonteCarlo       -    86.4    92.0    96.7    99.2    99.0    99.2    99.1    99.4    98.9
Lookahead     13.7       -    60.5    68.9    79.4    81.3    81.8    85.5    92.2    93.6
Strategist     8.0    39.6       -    58.4    69.2    68.8    72.0    72.7    89.2    91.6
Builder        3.3    31.1    41.6       -    59.2    59.8    65.8    66.0    86.4    86.5
Smart          0.8    20.6    30.8    40.8       -    55.1    54.8    55.3    80.0    84.1
Defensive      1.1    18.6    31.1    40.2    44.9       -    56.2    51.6    79.4    85.5
Greedy         0.8    18.1    28.1    34.2    45.2    43.8       -    52.0    79.6    83.0
LoopHunter     0.9    14.5    27.4    34.1    44.7    48.4    47.9       -    81.5    82.6
Random         0.6     7.8    10.8    13.6    20.0    20.6    20.4    18.5       -    51.6
FirstMove      1.1     6.4     8.3    13.5    15.9    14.5    17.0    17.4    48.4       -
```

MonteCarlo led this field at 96.7%, costing about 30 ms/move; Lookahead
was second at roughly 1/50 the cost. Strategist beat Smart 69–31.
First-player score was 50.3% ±0.5, consistent with no advantage in this run.

### Tuning the line evaluation

Strategist variants, one weight changed at a time, 1000 games per pairing
(the configs listed every variant, like `weights.toml`).

- Round 1 (13 variants around the first guess): `forced_end` mattered most
  (0: 30.7%, 2.5: 56.6%, the best); `ready_end` hurt (2.5: 35.3%, 0:
  54.7%); `joinable = 1` and `open_end = 3` helped a little;
  `closable_loop` and `opponent` were within noise (±0.9%).
- Round 2 (12 variants around round 1's best): every variant within 2.5%
  of the others, the first guess last (46.5%), about 30 Elo below the best,
  which became `Weights::BALANCED`: `open_end` 3, `forced_end` 3,
  `ready_end` 0.5, `closable_loop` 0.25, `joinable` 1, `opponent` 1.

### How MonteCarlo should spend its playouts

- `search.toml` (flat playouts, 200 games per pairing): strength follows
  the budget, width × playouts: 10×32 scored 68.1%, 16×16 64.5%, 10×16
  60.3%, then 6×16, 16×8 and 24×8 about 55%. Eight playouts a move are too
  few, at any width.
- `halving.toml` (200 games per pairing): successive halving beats flat
  playouts on the same budget: 10×16 halving beat flat 10×16 54–46, and
  16×16 halving (30 ms a move) beat flat 10×32 (40 ms) 58.5–41.5. 24×16
  halving was the strongest (65%, 42 ms a move). MonteCarlo plays 16×16
  halving by default.
- Lookahead's width hardly matters (4, 8 or 16: 38–42% in the same field).

## Performance

The engine got about twice as fast without changing a single move
(`cargo run --release -p tantrix-ai --example seeded_games` prints a
checksum of every game's history and scores, unchanged throughout):

| | Before | After |
|---|---|---|
| Seeded games, the six original AIs, 200 per pairing | 2.36 s | ≈1.1 s |
| Their legal moves alone (replay) | 1.43 s | ≈0.4 s |
| Their scoring alone (168,000 boards) | 0.66 s | ≈0.35 s |
| MonteCarlo 6×16 flat, per move | 24.4 ms | 11.4 ms |

- Legal moves and scoring look positions up in a dense `Grid` of the
  board's bounding box, built once per pass, instead of binary searches of
  the sorted board (about 800 per legal-move pass).
- Tiles are matched against spaces on packed `Edges` (two bits a
  direction): one compare per rotation.
- `Board::score_after` scores one color after a move by tracing that color
  alone, exact without copying the board; `Board::indexed` builds the
  lookup grid once for all of a decision's candidates. Greedy playouts keep
  the scores up to date move by move instead of rescoring the board.

Legal-move generation is now about half of MonteCarlo's time.

## Unimplemented options

- Incremental legal moves: update affected contour spaces and controlled sides.
- Information-set MCTS; stronger or truncated playout policies.
- Exact endgame search once the bag is empty.
- Time-budgeted search; current Expert uses a fixed playout count.
- Automated weight tuning (SPSA/CMA-ES) and richer line-end features.
- Sequential stopping or challenger gauntlets to reduce tournament games.
- 3–4-player tournaments (AIs support them; the runner deals two players).
- Machine-readable results (`--json`).
