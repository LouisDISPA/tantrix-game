# Online multiplayer

Rooms are server-authoritative. `tantrix-room` owns rules and orchestration;
native Axum or a Cloudflare Durable Object provides sockets and storage.
`tantrix-proto` defines intents/snapshots; `tantrix-net` transports them in
browsers and desktop clients. See [Worker routing/storage](../cloudflare/README.md).

## Player flow

1. **Create a room** asks the server to allocate a lobby and six-letter code.
   Share the code or the permanent `/online/game/{ID}` link. A typed code is
   resolved once; the page and every reconnect then use the permanent ID.
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
- Same browser, same player: reloading or coming back later resumes your
  seat if nobody took it over. A second tab displaces the first, which keeps
  its board and offers **Resume here** without automatically reconnecting.
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

## In-game recovery

The room page owns one connection task; replacing a socket leaves the game
context, board, hands and camera mounted. After a transport failure, the last
snapshot remains visible with play disabled. Panning, zooming, score inspection,
themes and leaving still work. Selections, drags and room confirmation dialogs
close. Commands are never replayed on a replacement socket.

Recovery joins the captured server and room with the existing session identity,
using its permanent game ID. It tries up to three times after 1, 2 and 4 seconds. Each attempt
has 20 seconds to receive a fresh snapshot, bounded by 60 seconds total from the
first loss. Brief connections keep consuming that allowance; 30 seconds of stable
connection renews it. **Retry** starts a new bounded cycle after exhaustion.
Leaving the page cancels both the active socket and scheduled retries.

Welcome and lobby information only update identity during recovery. A fresh
`GameStarted` or `GameEnded` snapshot restores play or results, recalculating seat
ownership. A displaced player resumes as a spectator with an explanation. Results
remain available after later socket closure.

Both transports send keepalive pings every 30 seconds and allow 15 seconds for a
pong. A missed response enters the same recovery flow; suspended timers never
accumulate ping or retry bursts. The exact shared `OPENED_ELSEWHERE` close reason
becomes a typed transport event and stops recovery until **Resume here** is chosen.

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
example `https://tantrix.example.com`) so session cookies use the Secure flag.
Room endpoints do not validate the browser's Origin header.

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

Creation uses `POST /api/rooms` with a random request UUID. The server derives
one permanent ID from the private session identity and that request, making
retries safe after lost responses. The client retains pending requests per
server across reloads. Codes are generated on the server with rejection sampling;
allocation stops after ten collisions and returns a retryable 503.

`GET /api/rooms/code/{CODE}` resolves an invitation. `GET /api/rooms/{ID}`
gets the current reference; `WS /room/{ID}` joins that exact game. Both servers
validate permanent IDs before accepting sockets. The
socket sends `RoomInfo` with the current code, then the normal Hello/Welcome
exchange. Missing games reject Welcome with `RoomNotFound`; expired lobbies
use `RoomExpired` (HTTP APIs return 410).

## Inactivity and persistence

`tantrix_room::lifecycle` owns the shared policy and recoverable allocation steps.
After 24 hours with no joined connection, an unstarted lobby drops its contents
and retains an expired tombstone; started/finished games preserve their state,
bag and history, and release only their short code. A later visit to a saved
game allocates a fresh code when needed. Allocation failure does not prevent
resuming a started game by its permanent link. Disconnected seats do not keep
invitations alive; joined spectators do. Native restarts begin an inactivity
period for rooms previously marked connected, while retaining existing deadlines.

Native SQLite stores records and uniquely claimed codes in `online_games` and
`online_codes`; a one-minute sweep cleans due records. The Worker keeps one Room
object per permanent ID and one RoomCodes object per invitation code, with room
alarms for cleanup. Hibernating socket attachments preserve joined presence.
Access also checks expiry, so delayed cleanup never makes an expired code valid.
Code reservations are persisted before being published, and releases compare
both game ID and a unique claim nonce. Pending reservations and releases survive
storage failures and are safe to repeat.

This is a breaking storage/protocol change: old code-only rooms are not migrated.

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
