# AI players and the tournament

`tantrix-ai` holds the AIs, `tantrix-tournament` ranks them. Every AI
implements `TantrixAi` (see [game.md](game.md#ai-service)) and chooses from
an `AiView`: the board, every hand (face up, as in the real game), whose
turn it is and at which step, how many tiles are left in the bag, and the
legal moves. Never the draw order: an AI that plays ahead deals the unseen
tiles into a bag of its own (`AiView::determinize`).

## The AIs

`all_ais()`, in order. Elo and speed are from the championship below.

| AI | How it chooses | Elo | ms/move |
|---|---|---|---|
| `FirstMove` | The first legal move | 1172 | 0.000 |
| `Random` | A random legal move (`RandomAi::seeded` for repeatable games) | 1199 | 0.000 |
| `Greedy` | The move that most raises its own score | 1423 | 0.003 |
| `Defensive` | Its score after the move, less the best opponent's | 1445 | 0.011 |
| `LoopHunter` | Loops weigh 3× their tiles, lines 1× | 1421 | 0.007 |
| `Smart` | Own gain ×1000, opponent's gain ×−100, −50 next to the opponent's path ends; the app's AI | 1457 | 0.012 |
| `Builder` | The line evaluation one move ahead, opponents ignored | 1527 | 0.14 |
| `Strategist` | The line evaluation one move ahead: its value less the best opponent's | 1593 | 0.16 |
| `Lookahead` | Strategist's 8 best moves, each answered by the next player's best reply | 1686 | 0.56 |
| `MonteCarlo` | Strategist's 16 best moves played out to the end on determinized bags, by successive halving | 2076 | 29.7 |

The app offers four of them as difficulty levels, per AI seat (`Difficulty`
in `tantrix-app/src/state/config.rs`): Easy `Smart`, Medium `Strategist`
(the default), Hard `Lookahead`, Expert `MonteCarlo`.

All are deterministic but `Random` and `MonteCarlo`, which draw from a
seeded generator once `with_seed` gives them one (the tournament does, per
game), so a seed always plays the same game.

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

Ranks the legal moves by the line evaluation, keeps the best `width` (8),
and answers each with the best next move by the same evaluation: the same
player's if the move leaves them forced spaces to fill or their free move
to play, else the next player's (whose hand is known). The tile drawn after
the move is unknown, so it is left out.

### MonteCarlo

Ranks the legal moves by the line evaluation, keeps the best `width` (16),
and plays them out to the end of the game, `playouts` (16) times each on
average. Each playout deals the unseen tiles into the bag in a random order
(`AiView::determinize`), and every move is played out on the same deals
(common random numbers), so luck of the draw weighs on all of them alike.
Playouts are greedy: every player picks the move that most raises their own
score (`IndexedBoard::score_after`), ties broken at random. A playout is
worth the final margin, plus 5 for a win. By successive halving (`halving`,
the default) the `width × playouts` playouts are spent over rounds, the
worse half of the moves dropped after each, so the best moves get the
most.

## The tournament

```bash
cargo run --release -p tantrix-tournament -- --help
cargo run --release -p tantrix-tournament -- --config tantrix-tournament/configs/championship.toml --headless
```

Every pairing of the AIs plays `games` games (1000 by default). Each deal
is played twice in a row with the AIs' seats swapped (`--no-mirror` turns
it off), so neither AI gets the better hands or turn order, and the
deal's luck cancels out within the pairing. Each game's seed is drawn from
the tournament seed before any game runs, deals the game and seeds both
AIs: the same seed gives the same results whatever the threads. Worker
threads take games round by round (the first game of every pairing, then
the second…), so the standings are balanced across pairings at any time:
stopping early (`s` in the TUI) leaves a fair sample, and the ETA holds.

It shows, per AI: Elo (a Bradley–Terry fit of every head-to-head record,
averaging 1500; one virtual draw per pairing keeps winless AIs finite),
score (wins + ½ draws) with its 95% margin, wins, losses, draws, average
points and margin, and the time it takes to choose a move (measured with
every thread busy). Then the head-to-head score of every pairing, and how
the first player to move fares.

- **TUI** (the default on a terminal): live leaderboard, score bars, score
  trends, latest games; pause, stop, next seed.
- **Headless** (`--headless`, or whenever stdout is not a terminal, as when
  an LLM or a script runs it): a progress line on stderr at every tenth of
  the games (and every 30 s), then the results as plain text on stdout.

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

Every run below is a config in `tantrix-tournament/configs/` (or was
one), on mirrored deals; ± is the 95% margin of a score.

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

- **MonteCarlo is the best by far**: 96.7% of the points, at least 86%
  against every AI, 99% against the old ones. It also costs the most:
  30 ms a move, 0.8 s a game.
- **Lookahead** is second at 1/50 of the cost; **Strategist** third, one
  move ahead only, and beats `Smart`, the app's AI, 69–31 in 0.16 ms.
- The old AIs that only count score are level (`Smart` 46.9%, `Defensive`
  45.4%, `Greedy` 42.8%, `LoopHunter` 42.4%): looking at what lines can
  still become is what pays.
- The first player to move scores 50.3% (±0.5): no first-move advantage.

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

The evaluation has reached what one move ahead can do with these features:
better play now comes from searching (Lookahead, MonteCarlo) or new
features (see the ideas below).

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

## Ideas not done yet

- **Incremental legal moves.** `Game` works out every contour space again
  after each move, though a move only changes the spaces around it (and
  the controlled sides through them). Legal moves are now about half of
  MonteCarlo's time.
- **Tree search.** Information-set MCTS: grow a tree over the
  determinized deals instead of flat playouts, so the opponents' replies
  are searched too.
- **Smarter playouts.** A cheap policy that extends its own line ends and
  keeps off the opponents' would make each playout tell more; or stop
  playouts early and score them with the line evaluation.
- **Exact endgame.** Once the bag is empty nothing is hidden: search the
  last few tiles exactly.
- **A time budget.** MonteCarlo playing for a set time rather than a set
  number of playouts suits the app, where devices differ: Expert plays it
  by playouts, and the browser e2e (`local-game.spec.ts`) only shows it
  answers, not how long it takes on a slow device.
- **Automatic tuning.** Drive headless tournaments from generated configs
  (SPSA, CMA-ES) instead of tuning by hand; try new features (how many
  tiles fit an end, not only whether one does).
- **Fewer games for the same answer.** Stop a pairing once its result is
  settled (a sequential test), or play one challenger against the field
  (a gauntlet).
- **3 and 4 players.** The AIs handle any number of players, but the
  tournament only deals 2-player games.
- **Machine-readable results** (`--json`) for scripts.
