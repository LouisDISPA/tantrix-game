# Architecture

How the crates fit together and how a game flows through them. Details live
in the domain docs: [engine](game.md), [AI](ai.md),
[multiplayer](MULTIPLAYER.md), [components](components.md),
[state](signals.md) and [Worker](../cloudflare/README.md).

## Crates

Arrows point at direct dependencies.

```mermaid
flowchart BT
    game["tantrix-game<br>rules, scoring, history"]
    ai["tantrix-ai"] --> game
    tui["tantrix-tui"] --> ai & game
    tournament["tantrix-tournament"] --> ai & game
    proto["tantrix-proto<br>messages, snapshots"] --> game
    room["tantrix-room<br>room engine, limits, retention"] --> proto & game
    net["tantrix-net<br>WebSocket clients"] --> proto & game
    app["tantrix-app<br>UI, native server"] --> ai & net & proto & game
    app -.->|server feature| room
    worker["tantrix-worker<br>SSR, Durable Objects"] --> app & room & proto
```

`tantrix-game` has no I/O and no UI. `tantrix-app` is one Dioxus crate built
several ways; platform code is gated with the `cfg` aliases in
`tantrix-app/build.rs`:

| Build | Features | Alias |
| --- | --- | --- |
| Browser client (wasm32) | `web`; `fullstack` for hydration | `web_client` |
| Native server, binary `tantrix` | `server` (`dx` adds it to fullstack builds) | |
| Desktop app | `desktop`; `bundle` for packages | `native_client`, `native_io` |
| Android app | `android` (uses `mobile`) | `native_client`, `native_io` on Android |
| Worker SSR | `worker`, compiled by `tantrix-worker` | |

## Local game

Every seat plays on this device (`GameDriver::Local`).

```mermaid
sequenceDiagram
    actor P as Player
    participant UI as Board and hands
    participant A as state/actions.rs
    participant G as GameState (wraps Game)
    participant AI as state/ai_service.rs
    P->>UI: tile, position, rotation
    UI->>A: click_tile / click_position / pick_rotation
    A->>G: Game::play_move(Placement)
    A->>G: advance_turn: skip stuck players
    alt an AI seat can move
        A-->>AI: phase WaitingForAi, the page effect runs run_ai_loop
        AI->>G: ai_choose, then play_move, for each AI seat in a row
        AI-->>A: loop ends, the page effect calls advance_turn
    else a human can move
        A-->>UI: phase SelectTile
    else Game::status() is not Playing
        A-->>UI: phase GameOver, results
    end
```

## Online rooms

Online play uses the same game screens with `GameDriver::Online`: the client
sends intents and mirrors the snapshots the room sends back. The client side
is the same for both servers:

- `state/room_api.rs` creates rooms and resolves codes over HTTP;
  `state/server_endpoints.rs` picks the server (the page's origin, a build's
  `TANTRIX_ROOM_URL`, or the native apps' saved address).
- `pages/online/driver.rs` owns one socket at a time and reconnects with
  jittered backoff (`state/online_state/recovery.rs`); `tantrix-net` carries
  the frames and keepalives.
- `hooks/online_game.rs` turns each snapshot into `GameState` and UI changes.

On the server, `tantrix_room::service::RoomService` runs each room: it owns
the room record and a `tantrix_room::engine::RoomEngine`, which turns socket
events into a room to save and messages to send; `room_logic` applies intents
to the game. Each server is a thin adapter: it supplies sockets, keepalives
and storage through `ServiceHost` and calls the service once per event
([rules](MULTIPLAYER.md#room-engine-rules), [events](MULTIPLAYER.md#room-servers)).

### Native server

`tantrix-app/src/server.rs` routes requests. `server/room.rs` holds the
`RoomManager`, which runs one actor task per open room and one task per
socket; `server/store.rs` keeps rooms in SQLite.

```mermaid
sequenceDiagram
    participant C as Client
    participant S as Axum (server.rs)
    participant R as Rooms (server/room.rs)
    participant E as RoomService
    participant DB as SQLite (server/store.rs)
    Note over C,S: the first response sets the session cookie
    C->>S: POST /api/rooms {request_id}
    S->>R: create game_id(session, request_id)
    R->>DB: insert the room, then its actor claims a code
    S-->>C: RoomReference {id, code}
    C->>S: WS /room/{id}?v=ONLINE_VERSION
    S->>S: handshake::check (Origin, ID, version)
    S->>R: open the room, start a socket task
    R-->>C: RoomInfo
    C->>R: Hello, then PlayMove, Rematch…
    R->>E: open / text / close events
    E-->>R: record to save, effects to deliver (ServiceHost)
    R->>DB: save
    R-->>C: Welcome, snapshots to every socket
```

### Cloudflare Worker

`cloudflare/src/index.js` serves health, robots, sitemap and static files and passes
everything else to the Rust module. `tantrix-worker/src/lib.rs` renders pages
and hands `/api/rooms*` and `/room/*` to `rooms_api.rs`. Each game is one
`Room` Durable Object (`room.rs`, named `game:{id}`); each invitation code is
one `RoomCodes` object (`catalog.rs`).

```mermaid
sequenceDiagram
    participant C as Client
    participant W as tantrix-worker
    participant RC as RoomCodes object
    participant R as Room object
    participant E as RoomService
    Note over C,W: the first response sets the session cookie
    C->>W: POST /api/rooms {request_id}
    W->>R: /create/{id}
    R->>RC: claim a code
    R-->>C: RoomReference {id, code}
    C->>W: WS /room/{id}?v=ONLINE_VERSION
    W->>R: forward the upgrade
    R->>R: handshake::check, load storage, accept a hibernatable socket
    R-->>C: RoomInfo
    C->>R: Hello, then PlayMove, Rematch…
    R->>E: open / text / close events
    R->>R: save to object storage
    R-->>C: Welcome, snapshots to every socket
```

## Room lifecycle

```mermaid
stateDiagram-v2
    direction LR
    [*] --> Lobby: POST /api/rooms
    Lobby --> Started: Start game
    state Started {
        [*] --> Playing
        Playing --> Results: no tiles left, no tile fits, or End game
        Results --> NextLobby: Play again (match archived)
        NextLobby --> Playing: Start game
    }
    Lobby --> Tombstone: 1 hour with nobody joined
    Tombstone --> [*]: deleted 1 day later
    Started --> [*]: deleted 3 days after the last joined connection
```

Policy: `tantrix_room::lifecycle`; constants and cleanup in
[retention](MULTIPLAYER.md#retention).

## Limits and logs

| What | Code | Details |
| --- | --- | --- |
| Room creation and code lookups per IP | `tantrix_room::rate_limit`; native `server/limits.rs`; Worker `[[ratelimits]]` in `cloudflare/wrangler.toml` | [Request limits](MULTIPLAYER.md#request-limits) |
| Socket Origin and online version | `tantrix_room::handshake` | [Running locally](MULTIPLAYER.md#running-locally) |
| Frame size, message rate, socket cap, members, nicknames | `tantrix_room::engine`, `tantrix_room::room_logic` | [Room engine rules](MULTIPLAYER.md#room-engine-rules) |
| Native socket timeouts and send queue | `tantrix-app/src/server/room.rs` | [Room engine rules](MULTIPLAYER.md#room-engine-rules) |
| Room expiry and deletion | `tantrix_room::lifecycle` | [Retention](MULTIPLAYER.md#retention) |
| Client keepalive and reconnects | `tantrix-net/src/heartbeat.rs`; `state/online_state/recovery.rs` | [In-game recovery](MULTIPLAYER.md#in-game-recovery) |
| Native logs: `RUST_LOG`, `TANTRIX_LOG_FORMAT`, `room` and `socket` spans | `server.rs` (`setup_logging`), `server/room.rs` | [Server logs](MULTIPLAYER.md#server-logs) |
| Worker logs: errors only | `console_error!` in `tantrix-worker`; `[observability]` in `cloudflare/wrangler.toml` | [Worker](../cloudflare/README.md#logs) |
