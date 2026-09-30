# Local patches to `matchbox_socket` 0.14.0

This is the published `matchbox_socket` 0.14.0 crate, vendored and wired in via
`[patch.crates-io]` in the workspace `Cargo.toml`. Everything is upstream except
the items below; drop this directory (and the `[patch]` entry) once upstream
offers an equivalent.

## 1. Bounded wait for ICE gathering (`src/webrtc_socket/wasm.rs`)

Upstream sends the SDP offer/answer only after `iceGatheringState` becomes
`complete`, with no timeout. If any configured ICE server never answers (an
unreachable STUN server, or a network path that silently drops its UDP
packets), Chromium keeps gathering until the STUN request times out, ~40 s, and
the other peer sits on "Joining room…" the whole time.

`wait_for_ice_gathering_complete` now gives up after `ICE_GATHERING_TIMEOUT`
(3 s), logs a warning, and continues with the candidates gathered so far. The
remaining ones are trickled to the peer by `complete_handshake`, exactly as
before. It also:

* re-checks the state after installing the state-change handler (the event
  cannot be missed if gathering finishes in between), and
* no longer `unwrap`s the internal channel send (a duplicate `complete`
  event used to panic).

Reproduced and covered by `e2e/tests/online-ice.spec.ts`, which adds an
unroutable STUN server to both browsers' ICE config.

## 2. `#![allow(deprecated)]`

Silences upstream's `try_next` deprecation warnings, which a path dependency
(unlike a registry one) prints on every build.

## 1b. Trickle late candidates from the offerer (`wasm.rs`)

Consequence of patch 1: the offer can now leave before gathering finished, so a
candidate found in the gap while the offerer waits for the answer would be
dropped (upstream installs `onicecandidate` only once the answer arrived). The
handler is now split into `trickle_local_candidates` and installed on the
offerer right after the offer is sent. Covered by the slow-STUN test in
`e2e/tests/online-ice.spec.ts`.
