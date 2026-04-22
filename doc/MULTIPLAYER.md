# Multiplayer — local testing guide

End-to-end walkthrough for running the online mode on one machine. Everything
happens between two browser tabs and a signaling server; no external infra.

## 1. Start the signaling server

We use upstream `matchbox_server` unmodified. Install it once (matching the
`matchbox_socket` version pinned in `tantrix-net/Cargo.toml`):

```bash
cargo install matchbox_server --version 0.12
```

Then run it:

```bash
matchbox_server
```

It binds `0.0.0.0:3536` by default and logs every WS connection to stdout.
Leave it running in its own terminal — it holds the WebRTC rendezvous for
the whole test session.

## 2. Start the app

In a second terminal:

```bash
dx serve --package tantrix-app
```

Dioxus will serve the web build at `http://localhost:8080` (check the
console for the exact URL).

## 3. Create a room (host tab)

1. Open `http://localhost:8080` in a browser window.
2. Click **Online**.
3. Enter a nickname and click **Host a new room**.
4. You land on the lobby. Note the 6-character room code (e.g. `ABCXYZ`) —
   it's the path matchbox_server uses for signaling (`ws://localhost:3536/ABCXYZ`).
5. Pick a color by clicking one of the color chips.

## 4. Join the room (guest tab)

Open a second browser window (private/incognito or a different profile
works too — it just needs its own storage). Two paths:

**Option A — paste the code:** open `http://localhost:8080`, click
**Online**, enter a nickname, type the room code, click **Join room**.

**Option B — share link:** the lobby shows a share URL of the shape
`/#/join/ABCXYZ`. Navigate to `http://localhost:8080/#/join/ABCXYZ` in the
second tab; you'll be asked for a nickname then dropped straight into the
lobby.

Pick a different color in the guest tab. Both tabs should now show both
players seated.

## 5. Start the game

In the **host** tab, the **Start game** button lights up once at least two
colors are seated. Click it. Both tabs switch to the board view and the
host's authoritative `Game` drives turns from there on. Play a tile; the
guest tab should receive the state update and render the move within a
frame or two.

## Troubleshooting

- **"signaling closed before peer id was assigned"**: `matchbox_server`
  isn't running, or isn't reachable at `ws://localhost:3536`. Start it, or
  override via `TANTRIX_SIGNALING_URL=ws://... dx serve`.
- **Both tabs stuck on "Joining room…"**: the host tab closed before the
  guest connected. Matchbox rooms exist only while at least one WS is
  open; reopen the host tab first.
- **Host drops / game ends**: expected. Per the agreed spec, the room dies
  when the host disconnects; survivors see `GameEnded { HostLeft }`.
- **NAT / different machines**: `matchbox_server` relays only signaling.
  The actual data channels are direct p2p and may fail across symmetric
  NATs without a TURN server. For LAN tests this is irrelevant.

## Pointing at a remote server

Rebuild the app with the signaling URL baked in:

```bash
TANTRIX_SIGNALING_URL=wss://matchbox.example.com dx build --release --package tantrix-app
```

The constant lives in `tantrix-app/src/state/signaling.rs` and is read via
`option_env!` at compile time.
