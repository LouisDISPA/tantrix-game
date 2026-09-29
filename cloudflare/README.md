# Cloudflare Worker — Tantrix Signaling

This directory contains the Cloudflare Worker that replaces a self-hosted
`matchbox_server`. It serves two roles from a single entry point:

- **Static hosting** — serves the compiled Dioxus WASM app (HTML, JS, WASM,
  CSS, fonts) via the Workers Static Assets binding.
- **WebRTC signaling** — handles WebSocket connections for Matchbox-compatible
  SDP / ICE exchange so peers can discover each other and form a direct
  WebRTC DataChannel.

Once the DataChannel is established, all Tantrix game messages flow
peer-to-peer and the Worker is no longer involved.

## File layout

```
cloudflare/
  src/index.js     Worker entry point + Room Durable Object
  wrangler.toml    Cloudflare project configuration
  package.json     Dev dependency on wrangler CLI
```

## How requests are routed

```
Browser GET /                       → env.ASSETS (static app)
Browser GET /assets/*               → env.ASSETS (static app)
Browser WS  /signaling/{ROOM_CODE}  → Room Durable Object
Browser GET /health                 → 200 OK
```

The Worker's `fetch` handler checks three things in order:

1. **`/health`** — returns `200 OK`. Useful for uptime monitoring.
2. **`Upgrade: websocket` header + path matches `/signaling/{ROOM_CODE}`** —
   routes to the `Room` Durable Object for that room code. Returns `400` if
   no room code is present in the path.
3. **Everything else** — forwarded to `env.ASSETS`, which serves the bundled
   Dioxus frontend. `not_found_handling = "single-page-application"` is set
   so deep-links and client-side routes all get `index.html`.

## How the Dioxus app connects

`tantrix-app/src/state/signaling.rs` builds the WebSocket URL:

```
wss://{origin}/signaling/{ROOM_CODE}
```

On production (served from the same Cloudflare origin) this resolves
automatically from `window.location`. During local development, the
app defaults to `ws://localhost:8080/signaling/{ROOM_CODE}` — run a local
`matchbox_server` on port 8080 for that case.

To override at build time (e.g. point a `dx serve` dev build at the deployed
Worker):

```bash
TANTRIX_SIGNALING_URL=wss://tantrix-game.workers.dev/signaling \
  dx serve --features web
```

## Room Durable Object

Each room code gets its own `Room` Durable Object instance. The instance
lives for as long as at least one WebSocket is connected; Cloudflare may
evict it from memory while all peers are connected (hibernation), then
restore it when the next message arrives.

### Connect sequence

When a peer connects:

1. A `WebSocketPair` is created and the server half is accepted with
   `ctx.acceptWebSocket(server)` (hibernation API — avoids duration charges
   while idle).
2. A fresh UUID is generated as `peerId` and stored on the socket via
   `serializeAttachment({ peerId })` so it survives hibernation.
3. `{"IdAssigned": "<peerId>"}` is sent to the new socket.
4. `{"NewPeer": "<peerId>"}` is broadcast to every **existing** socket in the
   room. The new peer does **not** receive `NewPeer` for existing peers —
   this makes existing peers the WebRTC offerers, which matches
   `matchbox_socket`'s expected full-mesh flow.

### Signal forwarding

Client sends:
```json
{"Signal": {"receiver": "<target-peer-id>", "data": {"Offer": "..."}}}
```

The Durable Object looks up the socket whose attached `peerId` matches
`receiver` and forwards:
```json
{"Signal": {"sender": "<sender-peer-id>", "data": {"Offer": "..."}}}
```

`data` is forwarded verbatim and may contain `Offer`, `Answer`, or
`IceCandidate`. Unknown receivers are silently ignored.

### KeepAlive

`matchbox_socket` periodically sends the bare string `"KeepAlive"`. The
Durable Object ignores it and sends no response.

### Disconnect

On WebSocket close or error, `PeerLeft` is broadcast to all remaining sockets
in the room:

```json
{"PeerLeft": "<peerId>"}
```

The peer ID is not reserved. A reconnecting browser receives a fresh UUID.

## Local development

Install wrangler:

```bash
cd cloudflare && npm install
```

Build the frontend first (wrangler dev serves it from the assets directory):

```bash
dx bundle --web --release --package tantrix-app
```

Start the local emulator:

```bash
cd cloudflare && npx wrangler dev
```

Wrangler dev binds to `http://localhost:8787` by default. Open two tabs at
that address and join the same room code to exercise the full signaling flow
locally.

## Deployment

Authenticate once:

```bash
npx wrangler login
```

Build and deploy:

```bash
# from workspace root
dx bundle --web --release --package tantrix-app
cd cloudflare && npx wrangler deploy
```

Smoke test:

```bash
curl https://tantrix-game.workers.dev/health
# → OK
```

Then open two browser tabs at `https://tantrix-game.workers.dev`, create a
room in one tab, join with the code in the other, and confirm both reach the
Lobby screen.

## Cloudflare free-tier limits

| Resource | Free limit |
|---|---|
| Worker requests | 100,000 / day |
| Worker CPU | 10 ms / request |
| Durable Object requests | 100,000 / day |
| Durable Object duration | 13,000 GB-s / day |
| Static asset requests | Unlimited |

Every WebSocket upgrade and every incoming WebSocket message counts as a
Durable Object request. Signaling traffic is small (a handful of messages per
peer pair), so a room of 4 players uses roughly 10–20 DO requests to connect.
Game messages never reach the Worker.
