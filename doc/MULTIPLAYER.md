# Online multiplayer

Online games are server-authoritative. The room rules live in
`tantrix-room`; either the native Axum server or a Cloudflare Durable Object
owns a room, persists it, and broadcasts snapshots over WebSockets. Browsers
only send intents (`tantrix-proto`) and render what the room sends back
(`tantrix-net`). See [`cloudflare/README.md`](../cloudflare/README.md) for
Worker routing and storage notes.

## Player flow

1. **Create a room** generates a 6-letter code and opens the lobby. Share the
   invite link (`/join/{CODE}`) or the code.
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
- There is no protocol versioning: deploy the client and its room server
  together. A tab left open across a deploy should be reloaded.

## Running locally

Both backends support online play:

```bash
just dev          # native server at http://localhost:8080
just native-e2e   # Playwright suite against the native server
just worker-dev   # http://localhost:8787
just worker-e2e   # Playwright suite against wrangler dev
```

The native server saves rooms in `tantrix-rooms.sqlite3` in its working
directory. Set `TANTRIX_ROOM_DB` to choose another file. It runs as one process;
multiple replicas need shared room routing or coordination. For HTTPS behind a
reverse proxy, set `TANTRIX_PUBLIC_ORIGIN` to the browser-facing origin (for
example `https://tantrix.example.com`) so cookie security and WebSocket origin
checks use that origin.

## Why workers-rs (wasm32) and not the Emscripten target

Cloudflare's Emscripten target for Rust Workers mainly helps crates that need
a C toolchain or a libc. Everything here is pure Rust, and the Worker already
builds with `workers-rs` for server rendering, so the room Durable Object
lives in the same `wasm32-unknown-unknown` module rather than adding a second
toolchain.
