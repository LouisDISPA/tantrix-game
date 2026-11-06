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
   Anyone without a seat watches, and can take over the seat of a player who
   disconnected (after confirming), inheriting its hand and turn, so a
   dropped player doesn't stall the game. Seats of connected players can't
   be taken mid-game. The replaced player leaves the room, and watches if
   they come back.
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

Welcome's same-match lobby only updates identity during recovery. A fresh
`GameStarted` or `GameEnded` snapshot restores play or results, recalculating seat
ownership. A newer match's lobby returns the page to the lobby. A displaced player resumes as a spectator with an explanation. Results
remain available after later socket closure.

Both transports send keepalive pings every 30 seconds and allow 15 seconds for a
pong. A missed response enters the same recovery flow; suspended timers never
accumulate ping or retry bursts. The exact shared `OPENED_ELSEWHERE` close reason
becomes a typed transport event and stops recovery until **Resume here** is chosen.

## Playing again

A seated player chooses **Play again** on the results screen. Everyone returns
to the same lobby, keeping the URL, join code and connections. Connected players
keep their colors; offline seats are released. Spectators can pick a free color.
The existing **Start game** button deals a fresh game with at least two players.

The finished match's full `GameData`, seats and result are archived atomically
with the new lobby. Native SQLite uses `online_matches(room_id, match_id)`;
Worker storage uses `match:{match_id}` keys in the same Room object. Old histories
remain stored separately from the active room until the room is
[deleted](#retention). There is no history browser yet.
Match IDs on Start, Play, End and Rematch reject commands from an earlier match;
repeated rematch clicks do not restart the next game. Online API version: **3**.

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
It also makes the server take the last `X-Forwarded-For` address as the
client's for [request limits](#request-limits), so set it only behind a proxy
that appends that header. Room endpoints do not validate the browser's Origin
header.

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
retries safe after lost responses. The client retains a pending request per
server across reloads for 24 hours (`tantrix_proto::CREATE_RETRY_WINDOW_MS`),
then makes a new one. Codes are generated on the server with rejection sampling;
allocation stops after ten collisions and returns a retryable 503.

`GET /api/rooms/code/{CODE}` resolves an invitation. `GET /api/rooms/{ID}`
gets the current reference; `WS /room/{ID}` joins that exact game. Both servers
validate permanent IDs before accepting sockets. The
socket sends `RoomInfo` with the current code, then the normal Hello/Welcome
exchange. Missing games reject Welcome with `RoomNotFound`; expired lobbies
use `RoomExpired` (HTTP APIs return 410).

### Request limits

Invitation codes are short enough to scan, so both servers count room
requests per client IP address:

| Endpoint | Limit per client |
| --- | --- |
| `POST /api/rooms` | 10 per minute |
| `GET /api/rooms/code/{CODE}` | 30 per minute |

A client can spend a whole minute's allowance at once. Further requests get
HTTP 429 with `RateLimited`, and the online menu shows *Too many room
requests. Wait a minute and try again.* A pending creation keeps its request
ID, so a retry within the day still makes one room. IPv6 clients count per /64
network. Loopback clients (local development, browser tests) are not limited.
Permanent-ID requests are not counted; each socket has its own frame limit.

- **Native.** `tantrix_room::rate_limit` keeps a token bucket per client in
  memory, so a restart forgets the counts. The client is the socket's peer,
  or with `TANTRIX_PUBLIC_ORIGIN` set, the last `X-Forwarded-For` entry.
- **Worker.** The `ROOM_CREATION_LIMIT` and `CODE_LOOKUP_LIMIT` rate-limit
  bindings in `cloudflare/wrangler.toml`, keyed by `CF-Connecting-IP`. They
  must match `tantrix_room::rate_limit`. Cloudflare counts per location and
  approximately.

## Inactivity and persistence

`tantrix_room::lifecycle` owns the shared policy and recoverable allocation steps.
After 24 hours with no joined connection, an unstarted lobby drops its contents
and retains an expired tombstone; rooms that have started a match preserve their
state and archives, including between matches, and release only their short
code until [retention](#retention) deletes them. A later visit to a saved
game allocates a fresh code when needed. Allocation failure does not prevent
resuming a started game by its permanent link. Disconnected seats do not keep
invitations alive; joined spectators do. Native restarts begin an inactivity
period for rooms previously marked connected, while retaining existing deadlines.

Native SQLite stores records and uniquely claimed codes in `online_games` and
`online_codes`; a one-minute sweep cleans due records, and startup gives every
record without a deadline one. The Worker keeps one Room object per permanent ID
and one RoomCodes object per invitation code, with room alarms for cleanup.
Hibernating socket attachments preserve joined presence.
Access also checks expiry, so delayed cleanup never makes an expired code valid.
Code reservations are persisted before being published, and releases compare
both game ID and a unique claim nonce. Pending reservations and releases survive
storage failures and are safe to repeat.

This is a breaking storage/protocol change: old code-only rooms are not migrated.

### Retention

Room storage is bounded by inactivity. `RoomRecord::is_deletable` decides;
the periods are constants in `tantrix_room::lifecycle`:

| Record | Deleted |
| --- | --- |
| Expired lobby tombstone | 30 days after the lobby expired (`TOMBSTONE_TTL_MS`) |
| Room that started a match, with its match archives | After 90 days with no joined connection (`RETENTION_MS`) |

A joined connection resets the period. A tombstone outlives every creation
retry: the lobby expires no sooner than 24 hours after its request, when
clients have stopped retrying it, and the 30 days cover client clocks set back.
Desktop and Android v0.2.0 keep a pending request without a time limit; a retry
after its tombstone is gone makes a new empty lobby under the old ID.

Deletion first releases any invitation code, then removes the record. Native
deletes the `online_games` row and its `online_matches` rows in one transaction.
The Worker's room alarm calls `storage.deleteAll()`, which also clears the
alarm (compatibility date 2026-02-24 or later). A Room object saved before
retention existed is scheduled the next time it is loaded.

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
  answered with an error and change nothing. The native server drops frames
  over 64 KiB before they reach the room.
- **Rate limit.** A socket may send 20 frames at once, then 2 per second on
  average; one that sends faster is closed with *Too many messages.* and
  leaves the room as a disconnect would. The allowance is kept in memory, so
  a restart or a hibernated Durable Object refills it.
- **Socket cap.** A room keeps at most 64 sockets (twice the member limit).
  A new socket in a full room closes the oldest one that never said `Hello`
  (*Too many connections to this room.*).
- **Timeouts (native).** A socket must send something other than a keepalive
  within 10 seconds of opening, then some frame at least every 75 seconds,
  or it is closed with *Connection timed out.* A peer that stops reading for
  10 seconds is dropped. The Worker relies on the runtime and the socket cap.
- **Nicknames.** Control characters, text-direction overrides and invisible
  fillers are removed; a name with no visible character is refused. Setting
  the same name again saves and broadcasts nothing.
- **Restarts.** A room loaded from storage forgets members without a seat
  whose sockets are gone; seated players keep their seats.
