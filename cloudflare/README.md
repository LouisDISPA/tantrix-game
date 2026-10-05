# Cloudflare Worker — Tantrix

The Worker serves the Dioxus app and runs each online game in a room Durable
Object. The browser sends intents over one WebSocket to the Worker. Rust code
validates every intent, owns the complete `tantrix_game::Game`, persists it,
and broadcasts the resulting state. There is no peer-to-peer transport or
browser host.

## Request routing

| Request | Handler |
| --- | --- |
| Existing CSS, JS, Wasm, and images | Workers Static Assets, before the Worker runs |
| `GET /health` | Worker health response |
| `GET /robots.txt`, `GET /sitemap.xml` | Worker search-discovery responses |
| `WS /room/{CODE}?create=1` | Create a room in its Durable Object and connect its creator |
| `WS /room/{CODE}` | Connect to an existing room; unknown codes are rejected |
| Missing file-like path | Static Assets 404 |
| App routes such as `/`, `/game`, `/online`, `/join/{CODE}` | Rust Dioxus server rendering and hydration |

The Worker sends canonical `Link` headers for `/`, `/learn`, `/about`, and
`/online`. Other rendered pages, including room and seeded-game URLs, receive
`X-Robots-Tag: noindex`. Keep `SITE_ORIGIN` in `src/index.js` aligned with the
custom domain in `wrangler.toml` when changing domains.

The room code selects one Durable Object. The code is generated in the browser
when the player clicks **Create a room**. Only that first connection carries
`create=1`; reload and reconnect join the existing room. A creator disconnect
does not end the room or its game. Any seated player can start when at least
two seated players are connected.

## Room state

`tantrix-worker/src/room.rs` owns Durable Object sockets and storage; the
shared `tantrix-room` engine (`engine.rs`) decides what each socket event
does, exactly as on the native server, and `room_logic.rs` applies lobby and
game rules (see [Room servers](../doc/MULTIPLAYER.md#room-servers)). Each
room stores its roster, seats, bag, hands, board, turn tracker, move history,
and terminal state. A changed room is written to Durable Object storage
before it is committed and its new snapshot broadcast; if the write fails,
every socket is closed with *Room storage is unavailable.* and the object
reloads the room from storage on its next event. Each socket's attachment
holds its number, session token and whether it joined, so the engine is
rebuilt from storage and attachments after hibernation; only persistent
state is needed after an instance restart.

Players are identified by the `tantrix_session` cookie
(`tantrix-room/src/session_cookie.rs`): `HttpOnly`, `SameSite=Lax`, one
year, `Secure` on HTTPS. Server-rendered pages set it when a browser has
none; the room upgrade also sets it as a fallback. The room reads it from the
WebSocket handshake and stores it on the socket attachment, so the token
never appears in a message. Room sockets whose `Origin` is another site are
refused with 403. The object checks the room code and `Origin` with the same
function as the native server (`tantrix_room::handshake::check`).

Clients send a `Hello` (just a nickname), then intents such as `PickColor`,
`ReplaceSeat`, `StartGame`, and `PlayMove` (a tile placement: tile,
position, rotation; the server decides whether it fills a forced space).
The server answers with a
`Welcome`, broadcasts lobby or game snapshots, and rejects invalid intents.
Public player IDs are separate from session tokens. A reconnect with the
same cookie resumes the seat and receives the latest snapshot; an older
socket of that player is closed. Any number of members (up to 32) can be in a
room, sharing the four seats. `PickColor` only claims a free seat;
`ReplaceSeat` deliberately takes a held one (the UI confirms first). In the
lobby anyone may replace anyone; during a game only a member without a seat
can, and inherits the hand and turn. A seated player's disconnect keeps the
seat. Members without a seat who disconnect are removed. Newcomers can join
a running game; they watch until they take a seat.

The server validates turn ownership and the complete Tantrix move rules in
`tantrix-game`. The client uses snapshots to draw the board and to suggest
valid moves, but its local state cannot commit a move.

## Free plan behavior

One Durable Object coordinates each room, so there is no database query or
cross-object fan-out for a turn. Hibernatable WebSockets let an idle room
release Worker memory without closing its connections. The protocol sends a
full snapshot when state changes or a player reconnects, and never polls.
Clients send a `ping` every 30 s so proxies keep idle sockets open; the
runtime answers `pong` through the WebSocket auto-response without waking the
object, so keepalives cost no Durable Object duration. Static assets are
served before Worker code.
This keeps request and CPU use tied to actual player actions, while persisting
turns makes a room survive an isolate restart.

## Build, test, and deploy

The web client must be built before the SSR module: `tantrix-worker/build.rs`
embeds the client's generated `index.html`, including its hashed asset names.
`just worker-build` builds both. `just setup-worker` installs Wrangler from
`cloudflare/pnpm-lock.yaml`; Worker recipes run that setup automatically.

```bash
just worker-dev      # build, then serve locally at http://localhost:8787
just worker-e2e      # build, then run Playwright against wrangler dev
just worker-deploy   # build and deploy
```

The Worker uses the custom domain in `wrangler.toml`. After deployment, check
`/health`, then create a room and join it from a second browser context.
