# Cloudflare Worker

The Worker serves Dioxus SSR and static assets; one Durable Object per room owns the game, persists accepted changes and broadcasts snapshots. Browsers send intents over WebSockets. Shared rules and orchestration live in `tantrix-room`; the Worker adapter is `tantrix-worker/src/room.rs`. See [multiplayer](../doc/MULTIPLAYER.md) for protocol, sessions, seats and persistence.

## Routing

| Request | Handler |
| --- | --- |
| Existing assets | Workers Static Assets, before Worker execution |
| `GET /health` | Health response |
| `GET /robots.txt`, `GET /sitemap.xml` | Search discovery |
| `POST /api/rooms` | Idempotent server allocation |
| `GET /api/rooms/code/{CODE}` | Resolve a short invitation code |
| `GET /api/rooms/{ID}` | Current game reference |
| `WS /room/{ID}` | Join/reconnect by permanent game ID |
| Missing file-like path | Static Assets 404 |
| App routes | Rust SSR + browser hydration |

Canonical `Link` headers cover `/`, `/learn`, `/about` and `/online`; other rendered pages receive `X-Robots-Tag: noindex`. Keep `SITE_ORIGIN` in `src/index.js` aligned with the custom domain in `wrangler.toml`.

Each game has a permanent-ID Room object; each short code has a RoomCodes object owning its reservation. Room alarms release invitations after 24 hours without joined sockets. Unstarted lobbies expire; started games and results remain available by permanent link.

Room sockets hibernate; 30-second client pings receive automatic pongs without waking the object. State changes and reconnects send full snapshots; clients never poll. Storage succeeds before state is committed and broadcast. A failed save closes every socket and forces a reload from storage on the next event.

## Build, test, deploy

Install prerequisites from [development](../doc/development.md). `just setup-worker` installs the locked Wrangler dependencies; Worker recipes run it automatically.

```sh
just worker-e2e       # build client + SSR module, then run browser tests
just worker-deploy    # build client + SSR module, then deploy
```

The private `worker-client-build` recipe builds the production browser client with rebuilt std and silent panics. `worker-ssr-build` then runs `scripts/build-worker-ssr.sh`: install the matching `worker-build`, compile Rust SSR, validate shared asset names and package into `cloudflare/build/ssr/`. Build order matters: `tantrix-worker/build.rs` embeds the generated client HTML. CI runs this same build and browser test pipeline through `just worker-e2e`. The vendored `dioxus-server` patch must match Dioxus 0.8.0-alpha.3; see [`PATCHES.md`](../vendor/dioxus-server/PATCHES.md). The browser uses the published `dioxus-web` crate. In [`app()`](../tantrix-app/src/lib.rs), all root children share one RSX fragment to avoid alpha.3's mounted-event hydration bug; keep that boundary until the upstream renderer is fixed.

After a build, serve locally without rebuilding:

```sh
cd cloudflare
pnpm exec wrangler dev --port 8787
```

Static assets come from `target/dx/tantrix/release/web/public/`; `.assetsignore` excludes `index.html` so pages reach SSR. After deployment, check `/health`, then create a room and join from a second browser context.
