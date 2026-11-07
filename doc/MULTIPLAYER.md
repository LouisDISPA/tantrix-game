# Online multiplayer

Rooms are server-authoritative. Crates and request flows:
[architecture](architecture.md#online-rooms); Worker routing and storage:
[Worker](../cloudflare/README.md).

## Player flow

1. **Create a room** asks the server to allocate a lobby and six-letter code.
   Share the code or the permanent `/online/game/{ID}` link. A typed code is
   resolved once; the page and every reconnect then use the permanent ID.
2. Any number of people can be in the lobby; the four colors are seats.
   Take a free seat, or **Release my seat** to leave it for someone else. The
   lobby lists who is waiting for a seat. Clicking a seat someone else holds
   asks for confirmation, then takes it; they stay in the room without a
   seat. Any seated player can press **Start game** once at least two seated
   players are connected. There is no host role: the room belongs to nobody,
   and its creator can leave.
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
  a stale browser page; update an incompatible desktop app or server.
- Every room socket URL also carries the client's version (`/room/{ID}?v=3`),
  so direct game links and reconnects are checked too. A different version
  is accepted, then closed with a reason the client shows: *This app is out
  of date. Update or reload.* (or that the server needs updating). Clients
  from before the parameter existed send none; the server treats them as
  version 3 (`handshake::UNVERSIONED_CLIENT`), so they keep working until
  `ONLINE_VERSION` changes and are then refused the same way.

## In-game recovery

The room page owns one connection task; replacing a socket leaves the game
context, board, hands and camera mounted. After a transport failure, the last
snapshot remains visible with play disabled. Panning, zooming, score inspection,
themes and leaving still work. Selections, drags and room confirmation dialogs
close. Commands are never replayed on a replacement socket.

Recovery joins the captured server and room with the existing session identity,
using its permanent game ID. It tries up to three times after 1, 2 and 4
seconds, each shortened at random by up to half so clients dropped together by
a restart don't all return at once. Each attempt has 20 seconds to receive a
fresh snapshot, bounded by 60 seconds total from the first loss. Brief
connections keep consuming that allowance; 30 seconds of stable connection
renews it. **Retry** starts a new bounded cycle after exhaustion.
Leaving the page cancels both the active socket and scheduled retries.

Welcome's same-match lobby only updates identity during recovery. A fresh
`GameStarted` or `GameEnded` snapshot restores play or results, recalculating seat
ownership. A newer match's lobby returns the page to the lobby. A displaced
player resumes as a spectator with an explanation. Results remain available
after later socket closure.

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
repeated rematch clicks do not restart the next game.

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
that appends that header.

Room sockets refuse (HTTP 403) a browser `Origin` from another site, so other
pages can't join rooms with a visitor's session cookie. The native server
allows `TANTRIX_PUBLIC_ORIGIN`, or, when it is unset, any page on the host the
request names (local development); behind a proxy that rewrites `Host`, set
it. The Worker allows its request URL's origin. Upgrades without `Origin`
(the desktop and Android apps) are allowed. A web client served from another
origin than its room server (a `TANTRIX_ROOM_URL` build) is refused.

Rooms deal with the OS-seeded thread RNG and retain no seed. Saves retain
the bag and history; snapshots expose neither. See
[history and persistence](game.md#history-seeds-and-persistence).

### Server logs

The native server logs to stdout, filtered by `RUST_LOG`
([`EnvFilter` directives](https://docs.rs/tracing-subscriber/0.3/tracing_subscriber/filter/struct.EnvFilter.html#directives);
default `tantrix_app=info,tower_http=info`). It writes compact text, or one
JSON object per line with `TANTRIX_LOG_FORMAT=json`.

Room work runs in a `room` span whose `room` field is the permanent game ID.
Each socket runs in a `socket` span with `room`, `conn` (a socket number) and,
once its `Hello` is accepted, `peer` (the player's public ID). Session tokens,
cookies, seeds and bags are never logged.

| Level | Events |
| --- | --- |
| info | Room created, loaded and stopped (`reason`); socket opened, joined, refused, rejected or closed (`reason`); lobby expired, invitation code released, room deleted |
| warn | Socket closed for a limit or failure: *Too many messages.*, *Too many connections to this room.*, *Connection is too slow.*, *Room storage is unavailable.*, or the client stopped reading. Rate-limited room requests (`client`, `limit`) |
| error | Storage errors, failed cleanup sweeps |
| debug | Each frame's size, cleanup sweeps |

A socket closed by the client, a lost connection or *Connection timed out.*
is info. To follow one room:

```sh
RUST_LOG='tantrix_app=info,[{room=<ID>}]=debug'   # debug for that room only
jq -c 'select(.span.room == "<ID>")' server.log   # saved JSON logs
```

## Room servers

`tantrix_room::service::RoomService` runs a room's server-side lifecycle
without I/O: loading, socket events, cleanup and code allocation. It owns
the room's `RoomRecord` and the `RoomEngine` serving its sockets. Both
servers call it once per event, passing the time, and implement its
`ServiceHost`: a `lifecycle::Catalog` (records and code claims), match
archives, and delivery of the engine's send, broadcast and close effects.

| Event | `RoomService` |
| --- | --- |
| Room read from storage, or created | `new`, then `load` |
| Reference request, socket upgrade, cleanup sweep or alarm | `inspect` |
| Socket accepted, frame, socket closed | `open`, `text`/`binary`, `close` |

The servers are thin adapters that keep only transport, timers and storage:

| | Native (`tantrix-app/src/server/`) | Worker (`tantrix-worker/src/room.rs`) |
| --- | --- | --- |
| Events | One actor task per room calls the service; one task per socket | Each request, socket event and alarm locks the object and calls the service |
| Sockets | Bounded queues to the socket tasks | Hibernatable Durable Object sockets; the engine's view of each is kept on its attachment |
| Keepalive and timeouts | The socket task answers `ping` and closes silent or slow peers | The runtime auto-response, without waking the object |
| Cleanup | A one-minute sweep inspects due rooms | A room alarm at the record's deadline |
| Storage | SQLite (`store.rs`) | Durable Object storage |

Creation uses `POST /api/rooms` with a random request UUID. The server derives
one permanent ID from the private session identity and that request, making
retries safe after lost responses. The client retains a pending request per
server across reloads for an hour (`tantrix_proto::CREATE_RETRY_WINDOW_MS`),
then makes a new one. Codes are generated on the server with rejection sampling;
allocation stops after ten collisions and returns a retryable 503.

`GET /api/rooms/code/{CODE}` resolves an invitation. `GET /api/rooms/{ID}`
gets the current reference; `WS /room/{ID}` joins that exact game. Both servers
check the socket's origin, permanent ID and version (`tantrix_room::handshake`)
before it reaches the room. The
socket sends `RoomInfo` with the current code, then the normal Hello/Welcome
exchange. Missing games reject Welcome with `RoomNotFound`; expired lobbies
use `RoomExpired` (HTTP APIs return 410).

### Room engine rules

`RoomEngine::run` applies these rules on both servers:

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
ID, so a retry within the hour still makes one room. IPv6 clients count per /64
network. Loopback clients (local development, browser tests) are not limited.
Permanent-ID requests are not counted; each socket has its own frame limit.

- **Native.** `tantrix_room::rate_limit` keeps a token bucket per client in
  memory, so a restart forgets the counts. The client is the socket's peer,
  or with `TANTRIX_PUBLIC_ORIGIN` set, the last `X-Forwarded-For` entry.
- **Worker.** The `ROOM_CREATION_LIMIT` and `CODE_LOOKUP_LIMIT` rate-limit
  bindings in `cloudflare/wrangler.toml`, keyed by `CF-Connecting-IP`. They
  must match `tantrix_room::rate_limit`. Cloudflare counts per location and
  approximately.

### Room events

The engine turns each socket event into a `Step`: an optional room to
persist and send, broadcast or close effects. The service carries it out
with `RoomEngine::run`, the same on both servers:

- **Save first.** A changed room is saved, with the archive of a match it
  just finished, before the step is committed and before anyone hears of
  it. If the save fails, nothing is committed and every socket is closed
  with *Room storage is unavailable.*; the room is loaded from storage
  again for the next visitor. A failed presence save, cleanup or
  allocation does the same.
- **Presence.** After each event the record notes whether a joined socket
  remains, and is saved only when that changed. Loading a room always
  saves it, which schedules its cleanup.
- **Expiry and deletion.** A lobby that expires closes its sockets with
  *Lobby expired.* and then turns new ones away; a room that retention
  deletes closes them with *Room deleted.*
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

`tantrix-room/src/engine/model_tests.rs` checks these rules, all but the
native timeouts, after random event sequences, failed saves, restarts and
hibernation included.

## Inactivity and persistence

`tantrix_room::lifecycle` owns the shared policy and recoverable allocation steps.
After an hour with no joined connection, an unstarted lobby drops its contents
and retains an expired tombstone; rooms that have started a match preserve their
state and archives, including between matches, and release only their short
code until [retention](#retention) deletes them. A later visit to a saved
game allocates a fresh code when needed. Allocation failure does not prevent
resuming a started game by its permanent link. Disconnected seats do not keep
invitations alive; joined spectators do. A native restart starts an inactivity
period for every room saved as connected, and keeps existing deadlines.

Native SQLite stores records and uniquely claimed codes in `online_games` and
`online_codes`; a one-minute sweep cleans due records. The Worker keeps one Room
object per permanent ID and one RoomCodes object per invitation code, with room
alarms for cleanup. Hibernating socket attachments preserve joined presence.
Access also checks expiry, so delayed cleanup never makes an expired code valid.
Code reservations are persisted before being published, and releases compare
both game ID and a unique claim nonce. Pending reservations and releases survive
storage failures and are safe to repeat.

### Retention

Room storage is bounded by inactivity (`tantrix_room::lifecycle`). A joined
connection restarts every clock below.

| Room | After 1 hour with nobody joined (`IDLE_TTL_MS`) | Deleted |
| --- | --- | --- |
| Lobby, no game dealt | Expires: code freed, only a tombstone left | 1 day after it expired (`TOMBSTONE_TTL_MS`) |
| Started a match | Code freed; resumable by permanent link | With its match archives, after 3 days with nobody joined (`RETENTION_MS`) |

A tombstone outlives every creation retry: clients retry a pending request
for an hour at most, and its lobby cannot expire sooner. The extra day covers
client clocks set back.

Deletion first releases any invitation code. Native deletes the record and its
archives in one transaction; the Worker's room alarm calls `storage.deleteAll()`,
which also clears the alarm (compatibility date 2026-02-24 or later).
