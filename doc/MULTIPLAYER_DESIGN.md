# Online multiplayer — design notes

Companion to `MULTIPLAYER.md` (which covers running it locally). This file
explains what's there, where it lives, and why each major choice was made.

## What was built

Tantrix is now playable peer-to-peer over WebRTC for 2–4 players in the
same browser tab pair, in any modern browser. One peer is the **host**:
it runs the only authoritative `Game` and validates every move. Other
peers send move *intents* and render whatever snapshots the host
broadcasts back. Signaling — the brief SDP/ICE handshake that lets the
peers find each other — runs through an unmodified
[`matchbox_server`](https://github.com/johanhelsing/matchbox/tree/main/matchbox_server)
binary; no custom server code is part of this repo.

## Crate layout

```
tantrix-game/      # unchanged game rules — gained serde derives only
tantrix-proto/     # NEW. wire types (no transport, no game logic)
tantrix-net/       # NEW. matchbox_socket glue + host/client state machines
tantrix-app/       # Dioxus UI — added /online, /join/:code, /online/:code/:role
                   # and the lobby + online-game pages.
```

Each crate has a single job. `tantrix-proto` is pure data — anyone
wanting to embed Tantrix in another transport can depend on just this
crate. `tantrix-net` knows how to open a matchbox socket and run a
session loop, but doesn't touch the DOM. `tantrix-app` is the only
crate that knows about Dioxus.

## Connection lifecycle

1. **Host clicks "Host a new room"** in `OnlineEntry`
   (`tantrix-app/src/pages/online/entry.rs`). The client generates a
   6-character room code (`tantrix-net::generate_room_code`, alphabet
   `A-Z` minus `I`/`O`/`Q` for legibility) and navigates to
   `/online/CODE/host`.
2. `OnlineRoom` (`pages/online/root.rs`) calls
   `tantrix_net::open_host_session(url, nickname)` and spawns the
   returned future via Dioxus's `spawn`. The session opens a
   reliable-only data channel against `wss://signaling/CODE`.
3. **Guest visits the share link** `/join/CODE`. `JoinRoom` validates
   the code shape, then redirects to `/online/CODE/client`. The same
   `OnlineRoom` runs, but it calls `open_client_session` instead.
4. Matchbox introduces the peers. The host has no peer to talk to until
   the first guest connects; the guest's session waits for its first
   `PeerState::Connected` event and treats that peer as the host.
5. The guest sends `PeerMessage::Hello { nickname, proto_version }`.
   The host replies with `HostMessage::Welcome` (or with `Welcome`
   carrying a `RejectCode` like `LobbyFull`/`IncompatibleVersion`),
   then broadcasts a `LobbyUpdate` to everyone.
6. Each player picks a color; the host's `Start game` button enables
   once at least two seats are taken. On click, the host constructs a
   `Game`, broadcasts `GameStarted { snapshot }`, and the UI flips to
   the play view.
7. Every move follows the same pattern: client sends `PlayMove { m }`,
   host calls `Game::play_move` (`tantrix-game/src/lib.rs:673`), and
   either rejects with `Error { code: InvalidMove }` or rebroadcasts
   the new `StateUpdate { snapshot, last_move }`.

## Wire protocol (`tantrix-proto`)

Messages are serde-encoded as JSON for ease of debugging in DevTools.
Switching to a binary codec is a one-line change in `encode`/`decode`.

**Client → Host (`PeerMessage`)**

| Variant | Purpose |
|---|---|
| `Hello` | First message after the data channel opens; carries nickname + protocol version. |
| `PickColor { color: Option<Color> }` | Claim a color; `None` releases the seat. |
| `StartGame` | Host-only; transitions lobby → game. |
| `PlayMove { m: Move }` | Submit a move for the current turn. |
| `SkipTurn` | No legal moves available; pass. |
| `UpdateNickname { nickname }` | Edit display name from the lobby. |
| `RequestSnapshot` | Ask host to re-broadcast current state. |

**Host → All (`HostMessage`)**

| Variant | Purpose |
|---|---|
| `Welcome` | Reply to `Hello`; includes lobby + optional `RejectCode`. |
| `LobbyUpdate { lobby }` | Lobby changed (joined/seated/renamed). |
| `GameStarted { snapshot }` | Lobby → game transition. |
| `StateUpdate { snapshot, last_move }` | New authoritative state after a move/skip. |
| `GameEnded { reason, snapshot }` | Terminal state. Host stops sending after this. |
| `Error { code, message }` | Rejection of the previous intent. |

`GameSnapshot` carries the full board, all hands, the current player,
seat assignments, and the bag count (not contents). For Tantrix's small
game state this is ~1 KB JSON; sending the whole thing on every move is
simpler than a delta protocol and cheap enough that it's not worth
optimizing.

## UI layer

### Routes (`tantrix-app/src/route.rs`)

```
/                      MainMenu
/game                  LocalGame  (unchanged)
/online                OnlineEntry
/online/:code/:role    OnlineRoom (role = "host" | "client", typed as RoleParam)
/join/:code            JoinRoom   (deep link → auto-redirects to OnlineRoom)
```

Plain history routing — no `#` fragment. Share links look like
`http://localhost:8080/join/ABCXYZ`.

### Contexts

- **`GameCtx`** is shared by local and online games. It carries the
  reactive `GameState`, UI state, and three online-only signals:
  `online: Signal<Option<CommandSender>>` (set ⇒ moves are forwarded
  over the network instead of mutated locally), `my_color`, and
  `player_names`. `actions::execute_move` checks `online` and routes
  accordingly.
- **`OnlineCtx`** lives only inside `/online/...` pages. It owns the
  `OnlineState` (current phase + room metadata + last error) and the
  command-sender handle. The `StatusBar` HUD chip uses
  `try_consume_context::<OnlineCtx>()` so it shows connection state
  only when in an online game.

### Reuse

The online-game page (`pages/online/game.rs`) doesn't ship its own
board UI. It builds a `GameState` from the incoming `GameSnapshot`
via `GameState::from_snapshot` and renders the existing `GamePlay`
screen. Components like `Board`, `PlayerHand`, `RotationWheel`, and
`StatusBar` are unchanged — they just see a `GameCtx` with `online`
populated and `my_color` set, which gates `HandTile` interactivity to
the local user's seat only.

## Key design choices

### Host-authoritative, not lockstep
A single peer (the host) owns the only `Game` instance. Pros: no
desync detection, no rollback machinery, trivial cheating story (the
host validates every move). Con: if the host disconnects, the game
ends — there's no host migration. Acceptable for a turn-based board
game; would be wrong for a competitive real-time genre.

### `matchbox_server` unmodified
`matchbox_server` already does exactly what we need: clients connect
to `ws://host/<room-id>`, rooms are implicit and live only while at
least one peer is connected. We don't need a room-list endpoint
(share links cover discovery), heartbeats (matchbox handles
`KeepAlive`), or auth (the room code is the secret). Building our own
signaling server would have meant ~500 lines of axum to replace
~50 KB of well-tested code.

### History router, not hash router
Dioxus 0.7 routes natively, no `#` needed. Share URLs read more
naturally as `/join/CODE`. Static hosts that don't fall back to
`index.html` for unknown paths will break, but that's a one-line nginx
fix and worth the cleaner URLs.

### Reliable channel only
Tantrix is turn-based. Messages must arrive in order and exactly once.
Unreliable channels exist in matchbox (UDP-like) for things like live
cursor positions, but we have no use for them.

### Full snapshots over deltas
Each `StateUpdate` ships the whole `GameSnapshot` (~1 KB JSON). Deltas
would be smaller but require a separate `apply` path on the client and
extra defensive code for missed packets. With reliable ordered delivery
and tiny game state, the simpler option wins.

### Event-driven select loop
`tantrix-net::host` and `client` use `futures::select!` over the
command channel, the data channel, the peer-state stream, and the
signaling loop's terminal future. There is no polling delay: an
inbound packet is processed the same tick it arrives. Earlier drafts
slept 50 ms between iterations, which added up to ~150 ms of
turn-roundtrip latency.

### 6-character codes from `A-Z` minus `I`/`O`/`Q`
23⁶ ≈ 148 M codes — plenty for the concurrent-room scale we care
about — and excluding visually ambiguous glyphs makes verbal sharing
reliable.

### Auto-generated nicknames, persisted only on edit
On every room mount, `state::nickname::load_or_generate` returns the
last *user-edited* name from `localStorage`, or falls back to a fresh
`Adjective + Noun` random pick (40 × 40 = 1600 combos). Generated
names are intentionally **not** persisted — otherwise two browser tabs
would always start out with the same auto name, defeating the purpose.
The localStorage key is `tantrix-nickname-v2` (the v1 key is actively
deleted on load to clear an early bug's stale cache).

### Single `GameCtx` for both modes
We considered keeping a separate `OnlineGameCtx` to avoid widening
`GameCtx`, but the trade-off — every shared component (Board,
PlayerHand, RotationWheel, StatusBar, action functions) would need to
read two contexts and decide which to consult — wasn't worth it. The
three added signals on `GameCtx` are inert in local mode (`None` /
empty) and the indirection in `actions::execute_move` is one
`if`-statement.

### `matchbox_socket::ggrs` not used
GGRS is rollback netcode for fixed-tick real-time games. Tantrix is
turn-based; there's nothing to predict and rollback on. Using GGRS
would have added a mandatory frame loop, fixed-size input packing,
and per-frame state cloning — all for zero benefit. See the answer
log under commit `1f6c704` for the longer rationale.

## Roster model (tokens, spectators, kick)

The host tracks every room member via a **client-generated session
token** — a UUID v4 stored in `localStorage` keyed by room code — not
the ephemeral matchbox `PeerId`. The token ships inside
`PeerMessage::Hello` and lets the host rebind a reload across a new
matchbox socket. Seats are keyed by token and reserved for the lifetime
of the game; a disconnected player keeps their seat until the host
manually kicks them.

Four participant roles live on the wire (`ParticipantRole`):

- `Host` — the room owner.
- `Seated(Color)` — has claimed a color and will play this game.
- `Unseated` — joined but still picking a color (lobby only).
- `Spectator` — joined after `StartGame` fired, or was an `Unseated`
  peer when the host hit start.

`LobbyState` and `GameSnapshot` both carry a `Vec<Participant>` with
connection status so the host UI can render the participants drawer
and the Kick button. There is no hard player cap — the first 2–4
peers who pick colors play; everyone else spectates.

### Kick

Host-only. Dispatched via `SessionCommand::Kick(PeerId)` →
`PeerMessage::Kick { target }` → `HostMessage::Kicked { target, reason }`
broadcast. If the target held a seat mid-game the host calls
`Game::retire_color` to drop them cleanly from the turn cycle
(`tantrix-game/src/lib.rs`). A single remaining player ends the game.
Kicked peers receive `Kicked` with their own id, clear the stored
token, and tear down. Kick is not a ban — reopening the share link
mints a new token and joins as a spectator; that's intentional.

## Limits and open questions

- **Host drop ends the game.** No host migration. With deterministic
  lockstep we could rotate hosts; with host-authoritative snapshots
  it's much harder because the new host needs the full `Game` state
  reconstituted from the last snapshot, and the missing entropy (bag
  contents) is irrecoverable.
- **No TURN.** Only Google's public STUN servers are configured.
  Players behind symmetric NATs may fail to establish data channels.
  Add TURN by extending the `RtcIceServerConfig` in
  `tantrix-net::socket::open` when needed.
- **Disconnected seats block the turn.** A dropped player who never
  returns freezes the game on their turn; the host has to kick to
  move on. Adding a "waiting for X…" countdown in the UI would be a
  useful nudge.
- **Stale tokens are never reaped.** A flood of visitors could grow
  the `participants` map unboundedly over a long session. At our
  player counts it's not a practical concern; if it becomes one,
  purge disconnected spectators after N minutes.
- **No room enumeration.** Discovery is share-link only; matchbox
  doesn't expose a room list and we haven't built one. A public lobby
  page would mean adding a thin axum service.

## Where to look in the code

| Concern | File |
|---|---|
| Wire types | `tantrix-proto/src/lib.rs` |
| Host state machine | `tantrix-net/src/host.rs` |
| Client state machine | `tantrix-net/src/client.rs` |
| Socket + id-wait helper | `tantrix-net/src/socket.rs` |
| Room codes / nickname | `tantrix-net/src/roomcode.rs`, `tantrix-app/src/state/nickname.rs` |
| Session token (localStorage) | `tantrix-app/src/state/session_token.rs` |
| Routes | `tantrix-app/src/route.rs` |
| Online pages | `tantrix-app/src/pages/online/{root,entry,lobby,game,driver}.rs` |
| Online ↔ UI bridge | `tantrix-app/src/state/online_state.rs` |
| Move dispatch override | `tantrix-app/src/state/actions.rs::execute_move` |
| HUD with connection state | `tantrix-app/src/components/status_bar.rs` |
| Participants drawer + kick | `tantrix-app/src/components/participants_panel.rs` |
| Retire seated color | `tantrix-game::Game::retire_color` |
