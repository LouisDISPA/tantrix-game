# Online multiplayer

Rooms are server-authoritative. `tantrix-room` owns rules and orchestration;
native Axum or a Cloudflare Durable Object provides sockets and storage.
`tantrix-proto` defines intents/snapshots; `tantrix-net` transports them in
browsers and desktop clients. See [Worker routing/storage](../cloudflare/README.md).

## Player flow

1. **Create a room** opens a lobby with a six-letter code. Share the code or
   `/join/{CODE}` invite link.
2. Any number of people can be in the lobby; the four colors are seats.
   Take a free seat, or **Release my seat** to leave it for someone else. The
   lobby lists who is waiting for a seat. Clicking a seat someone else holds
   asks for confirmation, then takes it; they stay in the room without a seat. Any seated player can press
   **Start game** once at least two seated players are connected. There is no
   host role: the room belongs to nobody, and its creator can leave.
3. Players take turns; the server rejects out-of-turn or illegal moves.
   Anyone without a seat watches, and can take over any playing seat (after
   confirming), inheriting its hand and turn — e.g. when a player drops out
   and would otherwise stall the game. The replaced player watches instead.
4. Results stay available in the room after the game ends.

## Disconnects and joining

- Identity is a `tantrix_session` cookie (`HttpOnly`, `SameSite=Lax`) the
  server sets on the first page visit. Scripts can't read it, and it is never
  in a WebSocket message: the room reads it from the socket handshake. Public
  player IDs sent to others are separate.
- Same browser, same player: reloading, pressing **Retry** on the
  disconnected screen, or coming back later resumes your seat. Opening the
  room in a second tab takes over from the first tab.
- A seated player who drops keeps the seat, shown as *Offline*, until they
  come back or someone takes it over. In the lobby **Start game** waits for
  them. People without a seat who leave are removed.
- Newcomers can join at any time; after the game started they watch. A room
  accepts up to 32 members, a bound that only guards against scripted joins.
- `tantrix-proto::ONLINE_VERSION` identifies online compatibility; both
  servers expose it at `/api/online-version`. The online menu checks in the
  background and blocks Create/Join after a mismatch or failed check. Reload
  a stale browser page; update an incompatible desktop app or server. This
  preflight is not a version negotiation in the WebSocket handshake.

## Running locally

Both backends support online play:

```bash
just dev          # native server at http://localhost:8080
just test-e2e     # Playwright suite against the native server
just worker-e2e   # build and test the Worker with wrangler dev
```

The native server saves rooms in `tantrix-rooms.sqlite3` in its working
directory. Set `TANTRIX_ROOM_DB` to choose another file. It runs as one process;
multiple replicas need shared room routing or coordination. For HTTPS behind a
reverse proxy, set `TANTRIX_PUBLIC_ORIGIN` to the browser-facing origin (for
example `https://tantrix.example.com`) so cookie security and WebSocket origin
checks use that origin.

Rooms deal with the OS-seeded thread RNG and retain no seed. Saves retain
the bag and history; snapshots expose neither. See
[history and persistence](game.md#history-seeds-and-persistence).

## Room servers

`tantrix_room::engine::RoomEngine` handles socket events without I/O;
adapters provide sockets, keepalives and storage:

| | Native (`tantrix-app/src/server/`) | Worker (`tantrix-worker/src/room.rs`) |
| --- | --- | --- |
| Sockets | One task per socket, one actor task per room | Hibernatable Durable Object sockets; the engine's view of each is kept on its attachment |
| Keepalive | The socket task answers `ping` | The runtime auto-response, without waking the object |
| Storage | SQLite (`store.rs`) | Durable Object storage |

Before upgrading, both check the handshake with
`tantrix_room::handshake::check` (a valid room code, and an `Origin` of our
own site, else 400 / 403) and get the browser's identity with
`session_cookie::identify`. `create=1` for a code with a room gets 409; a
code with no room gets a rejected `Welcome` (`RoomNotFound`) and a close.

Events (`on_open`, `on_text`, `on_binary`, `on_close`) produce a `Step`: an
optional room to persist and send/broadcast/close effects. Both adapters use
`RoomEngine::run`:

- **Save first.** A changed room is saved before the step is committed and
  before anyone hears of it. If the save fails, nothing is committed and
  every socket is closed with *Room storage is unavailable.*; the room is
  loaded from storage again for the next visitor.
- **Slow sockets.** A socket the server cannot hand a message to (the native
  64-message queue is full, or the socket is gone) is closed with
  *Connection is too slow.* and leaves the room as a disconnect would.
- **Hello once.** A socket must say `Hello` before anything else (*say hello
  first*), and only once (*already joined*). A refused `Hello` (full room,
  bad nickname) gets a rejected `Welcome` and the socket is closed;
  anything else it sends is ignored, so a retry needs a new connection.
- **Frames.** Binary frames, frames over 8 KiB and invalid JSON are
  answered with an error and change nothing.
- **Restarts.** A room loaded from storage forgets members without a seat
  whose sockets are gone; seated players keep their seats.
