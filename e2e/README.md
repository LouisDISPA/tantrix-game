# Browser end-to-end tests

The tests start the Dioxus fullstack app on port 4173, including its embedded
`matchbox_signaling` server (the Cloudflare Worker in `../cloudflare` is **not**
exercised). They run in headless Chromium.

| Spec | What it covers |
| --- | --- |
| `local-game`, `full-game` (local) | A local match played to the empty bag and the scores screen. |
| `online`, `full-game` (online) | Host + guest browsers over a real WebRTC data channel, played to the end. |
| `online-flow` | The same, fully instrumented: both boards, turn owner, bag counter, roster and connection state are cross-checked after **every** move. Also a guest reloading mid-game and resuming its seat. |
| `online-join` | The join handshake repeated (`JOIN_ATTEMPTS`, default 5) — the reliability probe for "Joining room…" hangs. |
| `online-ice` | Adds an unroutable STUN server to both browsers' ICE config. Regression test for the ~40 s join stall (see `../vendor/matchbox_socket/PATCHES.md`). |
| `online-retry` | A guest with no host: Retry appears on the connecting screen, the attempt times out with a message, and Retry succeeds once a host exists. |
| `board-layout` | The End Game / zoom controls never cover a legal board position (desktop, small laptop, phone widths). |

## Setup (once)

Install the repository's nightly Rust toolchain, the `wasm32-unknown-unknown`
target, Dioxus CLI (the version CI uses, matching the `dioxus` crate), Node.js,
and the browser:

```bash
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli --version 0.7.10 --locked
cd e2e
npm ci
npx playwright install chromium
```

## Running

From the repository root run `just e2e`, or from `e2e/`:

```bash
npx playwright test                              # everything
npx playwright test tests/online-flow.spec.ts    # one file
JOIN_ATTEMPTS=50 npx playwright test tests/online-join.spec.ts   # soak the handshake
npx playwright show-report                       # HTML report
```

Playwright starts and stops `dx run` itself; port 4173 must be free. The first
run compiles the wasm and server targets from cold (several minutes).

## Diagnosing a failing online test

Online tests wrap each browser page with `tests/diagnostics.ts`. On failure the
report (and `test-results/**/merged-timeline.log`) contains one time-ordered
log for **both** browsers:

* signaling WebSocket frames (SDP offers/answers are reduced to the candidates
  they advertise),
* `RTCPeerConnection` ICE gathering / connection state, every gathered
  candidate and candidate error, and every data-channel open / close / message,
* the app's own `log` output (`matchbox_socket` and `tantrix_net` at debug
  level, forwarded to the browser console by `tantrix-app/src/util/logger.rs`),
* console errors, page errors and failed requests.

Set `E2E_NO_STUN=1` to strip every ICE server from both browsers (host-only
candidates) when you need to rule STUN out.

## Chromium flag: real host IPs

`playwright.config.ts` launches Chromium with
`--disable-features=WebRtcHideLocalIpsWithMdns`. By default Chromium advertises
host candidates as `<uuid>.local` names that the other peer must resolve over
multicast DNS. That resolution silently fails on VPN interfaces (and in many
containers), leaving two browsers on the *same machine* unable to pair up even
though signaling worked. The flag only affects the test browsers; real players
keep the browser default, so a player on such a network can still fail to
connect. The app therefore times out and offers **Retry** instead of hanging.
