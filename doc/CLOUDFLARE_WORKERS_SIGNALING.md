# Cloudflare Workers Signaling Deployment

Plan and deployment notes for hosting Tantrix with:

- Dioxus web output as static assets.
- A JavaScript Cloudflare Worker as the only backend entry point.
- A Durable Object per room for Matchbox-compatible WebRTC signaling.
- Game traffic sent only over peer-to-peer WebRTC data channels.

This is intended as a production replacement for running an external
`matchbox_server`, while preserving `matchbox_socket` wire compatibility.

## Goals

- Keep the Dioxus app static-only on Cloudflare.
- Use Workers Static Assets for HTML, CSS, JS, WASM, images, and fonts.
- Use the Worker only for health checks and WebSocket signaling.
- Use Durable Objects only as room-scoped rendezvous points.
- Never relay Tantrix game messages through Cloudflare after WebRTC connects.
- Match upstream `matchbox_server` behavior for explicit room codes.

## Non-goals

- No Dioxus server functions on Cloudflare.
- No Dioxus SSR on Cloudflare.
- No room list or public matchmaking.
- No `?next=N` Matchbox matchmaking rooms.
- No persisted game state, turns, moves, tile state, lobby state, or chat.
- No reconnect slot reservation at the signaling layer.

## Architecture

```
Browser
  |
  | GET /, /assets/..., /wasm/...
  v
Cloudflare Worker Static Assets

Browser
  |
  | WebSocket /{ROOM_CODE}
  v
Cloudflare Worker
  |
  | Durable Object stub by room code
  v
Room Durable Object
  |
  | SDP / ICE signaling only
  v
Other peers

After signaling:

Browser <====== WebRTC DataChannel ======> Browser
```

The Durable Object does not understand Tantrix. It only assigns peer IDs,
announces peers, forwards WebRTC offers/answers/ICE candidates, ignores
keepalives, and announces disconnects.

## Matchbox Wire Protocol

The Worker must serialize JSON exactly like `matchbox_protocol` /
`matchbox_socket` 0.14.0.

Local reference sources:

- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/matchbox_protocol-0.14.0/src/lib.rs`
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/matchbox_socket-0.14.0/src/webrtc_socket/messages.rs`
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/matchbox_signaling-0.14.0/src/topologies/full_mesh.rs`
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/matchbox_server-0.14.0/src/topology.rs`
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/matchbox_server-0.14.0/src/state.rs`

### Server to client

```json
{"IdAssigned":"550e8400-e29b-41d4-a716-446655440000"}
{"NewPeer":"550e8400-e29b-41d4-a716-446655440001"}
{"PeerLeft":"550e8400-e29b-41d4-a716-446655440001"}
{"Signal":{"sender":"550e8400-e29b-41d4-a716-446655440001","data":{"Offer":"..."}}}
{"Signal":{"sender":"550e8400-e29b-41d4-a716-446655440001","data":{"Answer":"..."}}}
{"Signal":{"sender":"550e8400-e29b-41d4-a716-446655440001","data":{"IceCandidate":"..."}}}
```

### Client to server

```json
"KeepAlive"
{"Signal":{"receiver":"550e8400-e29b-41d4-a716-446655440001","data":{"Offer":"..."}}}
{"Signal":{"receiver":"550e8400-e29b-41d4-a716-446655440001","data":{"Answer":"..."}}}
{"Signal":{"receiver":"550e8400-e29b-41d4-a716-446655440001","data":{"IceCandidate":"..."}}}
```

Do not wrap these in an additional envelope. `matchbox_socket` expects
externally tagged serde enum JSON.

## Room Behavior

Use explicit rooms only:

```text
wss://<host>/<ROOM_CODE>
```

If no path is present, upstream Matchbox uses `world`. For Tantrix, prefer
requiring a room code and returning `400` for an empty room path, unless a
local test mode needs `world`.

### Connect

1. Worker validates that the request is a WebSocket upgrade.
2. Worker extracts the room code from `pathname`.
3. Worker routes to `env.ROOM.idFromName(roomCode)`.
4. Durable Object accepts the WebSocket with hibernation:

   ```js
   this.ctx.acceptWebSocket(server);
   ```

5. Durable Object generates a new UUID using `crypto.randomUUID()`.
6. Durable Object sends `IdAssigned(peerId)` to the connecting socket.
7. Durable Object sends `NewPeer(peerId)` to all existing sockets in the
   same room.
8. Durable Object attaches `{ peerId }` to the new socket with
   `serializeAttachment`.

Only existing peers receive `NewPeer`. This makes existing peers the
offerers, matching `matchbox_socket`'s expected full-mesh flow.

### Signal Forwarding

When a peer sends:

```json
{"Signal":{"receiver":"TARGET","data":{"Offer":"..."}}}
```

the Durable Object finds the WebSocket whose attached peer ID is `TARGET`
and sends:

```json
{"Signal":{"sender":"SENDER","data":{"Offer":"..."}}}
```

The `data` object is forwarded as-is. It may contain `Offer`, `Answer`, or
`IceCandidate`.

If the receiver is not found, matchbox_server logs and ignores it. The
Worker should follow that behavior: ignore unknown receivers, do not close
the sender.

### KeepAlive

`matchbox_socket` may send:

```json
"KeepAlive"
```

Ignore it and send no response.

### Disconnect

Follow upstream `matchbox_server` behavior:

1. Remove the peer immediately.
2. Do not reserve the peer ID.
3. Do not reserve a seat or slot.
4. Broadcast `PeerLeft(peerId)` to all remaining sockets in the room.
5. If the same browser reconnects, assign a new UUID.

Reconnect behavior above is only for signaling. The Tantrix application
may still have its own session-token and lobby logic over WebRTC after
the data channel forms.

## Durable Object Hibernation

Use Cloudflare's Durable Object WebSocket Hibernation API, not the standard
`ws.accept()` pattern.

Reason:

- Hibernation lets WebSocket clients stay connected while the Durable
  Object is evicted from memory.
- Idle hibernatable objects do not accrue duration charges.
- `serializeAttachment` and `deserializeAttachment` let us restore the
  peer ID after hibernation.

Important constraints:

- Attachments are limited; store only small metadata such as `{ peerId }`.
- Do not use `setInterval` or long-running timers in the room object.
- Reconstruct active peer maps from `this.ctx.getWebSockets()` when needed.
- Use storage only if a future feature truly needs persistence.

Cloudflare docs:

- Durable Objects overview: https://developers.cloudflare.com/durable-objects/
- WebSocket hibernation: https://developers.cloudflare.com/durable-objects/best-practices/websockets/
- Pricing: https://developers.cloudflare.com/durable-objects/platform/pricing/

## Worker Routes

Suggested routes:

| Route | Behavior |
|---|---|
| `GET /health` | Return `200 OK`. |
| `GET /{ROOM_CODE}` with `Upgrade: websocket` | Route to the room Durable Object. |
| Any other request | Serve static asset via `env.ASSETS.fetch(request)`. |

Do not use `/api/rooms` unless the UI later needs server-generated room
codes. Today the frontend already generates explicit room codes.

## Wrangler Configuration

Example `wrangler.toml`:

```toml
name = "tantrix-game"
main = "cloudflare/src/index.js"
compatibility_date = "2026-05-12"

[assets]
directory = "./target/dx/tantrix-app/release/web/public"
binding = "ASSETS"

[[durable_objects.bindings]]
name = "ROOM"
class_name = "Room"

[[migrations]]
tag = "v1"
new_sqlite_classes = ["Room"]
```

Adjust `assets.directory` to the actual `dx bundle --web --release`
output path in this workspace. Wrangler requires the Durable Object
migration even if the room object only uses in-memory socket state.

## Implementation Skeleton

```js
import { DurableObject } from "cloudflare:workers";

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (url.pathname === "/health") {
      return new Response("OK");
    }

    if (request.headers.get("Upgrade")?.toLowerCase() === "websocket") {
      const roomCode = url.pathname.slice(1);
      if (!roomCode) {
        return new Response("missing room code", { status: 400 });
      }

      const id = env.ROOM.idFromName(roomCode);
      return env.ROOM.get(id).fetch(request);
    }

    return env.ASSETS.fetch(request);
  },
};

export class Room extends DurableObject {
  async fetch(request) {
    if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
      return new Response("expected websocket", { status: 426 });
    }

    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    const peerId = crypto.randomUUID();

    server.serializeAttachment({ peerId });
    this.ctx.acceptWebSocket(server);

    server.send(JSON.stringify({ IdAssigned: peerId }));
    for (const socket of this.ctx.getWebSockets()) {
      if (socket !== server) {
        socket.send(JSON.stringify({ NewPeer: peerId }));
      }
    }

    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(ws, message) {
    if (typeof message !== "string") {
      return;
    }

    let request;
    try {
      request = JSON.parse(message);
    } catch {
      return;
    }

    if (request === "KeepAlive") {
      return;
    }

    if (!request.Signal) {
      return;
    }

    const sender = ws.deserializeAttachment()?.peerId;
    const { receiver, data } = request.Signal;
    if (!sender || !receiver) {
      return;
    }

    for (const socket of this.ctx.getWebSockets()) {
      if (socket.deserializeAttachment()?.peerId === receiver) {
        socket.send(JSON.stringify({ Signal: { sender, data } }));
        return;
      }
    }
  }

  async webSocketClose(ws, code, reason) {
    this.broadcastPeerLeft(ws);
    ws.close(code, reason);
  }

  async webSocketError(ws) {
    this.broadcastPeerLeft(ws);
  }

  broadcastPeerLeft(ws) {
    const peerId = ws.deserializeAttachment()?.peerId;
    if (!peerId) {
      return;
    }

    const event = JSON.stringify({ PeerLeft: peerId });
    for (const socket of this.ctx.getWebSockets()) {
      if (socket !== ws) {
        socket.send(event);
      }
    }
  }
}
```

## Deployment Steps

Install Wrangler locally in the project:

```bash
npm i -D wrangler@latest
npx wrangler --version
```

Authenticate:

```bash
npx wrangler login
```

Build the static Dioxus frontend:

```bash
dx bundle --web --release --package tantrix-app
```

Deploy:

```bash
npx wrangler deploy
```

Smoke test:

```bash
curl https://<worker-host>/health
```

Then open two browser tabs and join the same room code. The Worker should
only see WebSocket signaling traffic. Once the data channel connects,
Tantrix host/client messages flow peer-to-peer through `matchbox_socket`.

## Free-tier Notes

Workers Free and Durable Objects Free are sufficient for small test rooms,
but the limits matter:

- Worker requests: 100,000/day.
- Worker CPU: 10 ms/request.
- Durable Object requests: 100,000/day.
- Durable Object duration: 13,000 GB-s/day.
- Workers Free can only use SQLite-backed Durable Objects.
- Static asset requests are free and unlimited.

Every WebSocket connection and incoming WebSocket message can count toward
Durable Object request billing. Keep messages limited to WebRTC signaling.
Do not relay game state through the Durable Object.

Cloudflare references:

- Workers pricing: https://developers.cloudflare.com/workers/platform/pricing/
- Workers limits: https://developers.cloudflare.com/workers/platform/limits/
- Static Assets billing: https://developers.cloudflare.com/workers/static-assets/billing-and-limitations/
- Durable Objects pricing: https://developers.cloudflare.com/durable-objects/platform/pricing/

## Verification Checklist

- A new socket receives `IdAssigned` first.
- Existing sockets receive `NewPeer(newPeerId)`.
- The new socket does not receive `NewPeer` for existing peers.
- `Offer`, `Answer`, and `IceCandidate` pass through unchanged under
  `Signal.data`.
- Unknown receivers are ignored.
- `KeepAlive` is ignored.
- Closing a tab broadcasts `PeerLeft` to the remaining sockets.
- Reopening the tab assigns a fresh peer ID.
- No Tantrix `PeerMessage` or `HostMessage` is sent to the Worker after
  the WebRTC data channel is established.
