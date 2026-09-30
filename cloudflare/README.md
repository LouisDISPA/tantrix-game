# Cloudflare Worker — Tantrix hosting and signaling

This directory contains the Cloudflare Worker that hosts Tantrix in production
and replaces a self-hosted `matchbox_server`. It serves three roles from a
single entry point:

- **Server-side rendering** — every page is rendered by the Rust
  `tantrix-worker` module (Dioxus's fullstack renderer compiled to Wasm), so
  the browser gets the finished HTML and the web client *hydrates* it instead
  of rendering from scratch.
- **Static hosting** — serves the compiled web client (JS, Wasm, CSS) via the
  Workers Static Assets binding.
- **WebRTC signaling** — handles WebSocket connections for Matchbox-compatible
  SDP / ICE exchange so peers can discover each other and form a direct
  WebRTC DataChannel.

Once the DataChannel is established, all Tantrix game messages flow
peer-to-peer and the Worker is no longer involved.

## File layout

```
cloudflare/
  src/index.js     Worker entry point + Room Durable Object
  build/ssr/       SSR module generated from tantrix-worker (gitignored)
  wrangler.toml    Cloudflare project configuration
  package.json     Dev dependency on wrangler CLI

tantrix-worker/    Rust crate compiled to build/ssr (worker-build)
vendor/dioxus-server/
                   dioxus-server patched for wasm32 (see its PATCHES.md)
```

## How requests are routed

```
Browser GET /assets/*               → static assets (served before the Worker runs)
Browser GET /health                 → 200 OK
Browser WS  /signaling/{ROOM_CODE}  → Room Durable Object
Browser GET /missing-file.png       → env.ASSETS (404)
Browser GET /, /game, /online/...   → tantrix-worker (server-side rendering)
```

Requests for files that exist in the assets directory are answered by
Cloudflare's asset layer without invoking the Worker. The client's
`index.html` is *not* uploaded (the `.assetsignore` written by `just
worker-client` excludes it), so page requests fall through to the Worker,
whose `fetch` handler checks, in order:

1. **`/health`** — returns `200 OK`. Useful for uptime monitoring.
2. **`Upgrade: websocket` header + path matches `/signaling/{ROOM_CODE}`** —
   routes to the `Room` Durable Object for that room code. Returns `400` if
   no room code is present in the path.
3. **A path that looks like a file** (has an extension) — a missing static
   file, forwarded to `env.ASSETS` for its 404 rather than rendered as a route.
4. **Everything else** — an app route, rendered by `tantrix-worker`: Dioxus
   renders the route into dx's `index.html` together with hydration markers
   and the `window.initial_dioxus_hydration_data` payload, and the client
   (built with the `fullstack` feature) hydrates it. Unknown routes get the
   router's 404.

## How server-side rendering works

`tantrix-worker` is a `workers-rs` crate. Its `fetch` handler calls
`dioxus-server`'s axum router (`serve_api_application`) — the same renderer
the native fullstack server (`tantrix-app --features server`) uses — so the
Worker's HTML and hydration payload match what the client expects. Upstream
`dioxus-server` does not build or run on wasm32 (it renders on a pool of OS
threads), which is what the vendored patch fixes.

The client is built with `hydrate` enabled (`--features fullstack`), and a
hydrating client has no client-side-rendering fallback: **every HTML page must
come from the SSR module**. Deploy the client and the SSR module together
(`just worker-deploy` builds both).

Two build details keep them in step:

- `build.rs` embeds dx's generated `index.html` (it references the hashed JS
  bundle), so the client must be built first.
- A plain `cargo` build resolves `asset!()` (the stylesheets) to source paths
  on disk. `just worker-ssr` runs `dx tools assets` on the compiled Wasm to
  rewrite them to the same hashed `/assets/*.css` names the client build
  produced, and fails if a name has no matching file.

## How the Dioxus app connects

`tantrix-app/src/state/signaling.rs` builds the WebSocket URL:

```
wss://{origin}/signaling/{ROOM_CODE}
```

In the browser the base is always derived from `window.location`
(`https:` becomes `wss:`, anything else `ws:`), so the app talks to whichever
origin served it:

| Served from | Signaling endpoint |
|---|---|
| Production (`https://tantrix.louis-dispa.fr`) | This Worker, `wss://tantrix.louis-dispa.fr/signaling/{ROOM_CODE}` |
| `wrangler dev` (`http://localhost:8787`) | This Worker, `ws://localhost:8787/signaling/{ROOM_CODE}` |
| `dx serve` / `dx run --fullstack` (and the Playwright e2e run) | The `matchbox_signaling` server embedded in the fullstack binary (`tantrix-app/src/server.rs`), same origin, same `/signaling/{ROOM_CODE}` path |

No separate `matchbox_server` is needed for local development. The
`ws://localhost:8080/signaling` default in `signaling.rs` is only a fallback
for non-browser builds.

To override at build time (e.g. point a `dx serve` dev build at the deployed
Worker):

```bash
TANTRIX_SIGNALING_URL=wss://tantrix.louis-dispa.fr/signaling \
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
in the room, once per peer (a close followed by an error does not repeat it;
the `left` flag is kept in the socket attachment so it survives hibernation):

```json
{"PeerLeft": "<peerId>"}
```

The peer ID is not reserved. A reconnecting browser receives a fresh UUID.

## Local development

Besides the Rust toolchain and `dx`, you need Node.js/npm. The recipes install
the rest on first use: `worker-build` at the version matching the `worker` crate
(into `target/tools/`) and wrangler from `package-lock.json` (`npm ci`).

Build the client and the SSR module, then start the local emulator:

```bash
just worker-dev
```

`just worker-build` only builds; `just worker-client` and `just worker-ssr`
are its two steps (in that order). The e2e suite can run against the Worker
instead of the native server:

```bash
just worker-e2e
```

Wrangler dev binds to `http://localhost:8787` by default. Open two tabs at
that address and join the same room code to exercise the full signaling flow
locally; the app derives its signaling URL from that origin, so it talks to
the local Worker with no extra configuration.

The e2e suite and plain `dx serve` do not go through this Worker: they use the
`matchbox_signaling` server embedded in the fullstack binary.

## Deployment

Authenticate once:

```bash
npx wrangler login
```

Build and deploy:

```bash
just worker-deploy
```

Smoke test:

```bash
curl https://tantrix.louis-dispa.fr/health
# → OK
```

The Worker is served only from the custom domain configured in
`wrangler.toml` (`workers_dev = false`, `preview_urls = false`), so there is
no `*.workers.dev` URL.

Then open two browser tabs at `https://tantrix.louis-dispa.fr`, create a
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
