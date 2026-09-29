# Tantrix dev commands. Run `just` to list available recipes.
#
# Prerequisites:
#   cargo install just              (task runner)
#   cargo install dioxus-cli        (dx)
#   ngrok                           (for the tunnel recipe)
#
# The app is a Dioxus fullstack binary that serves both the client bundle
# and an embedded matchbox signaling server on a single port. No external
# matchbox_server is needed — the server binary (built with the "server"
# feature) hosts signaling at `/signaling/*` and the client everywhere else.

port := "8080"

default:
    @just --list

# Fullstack dev server with hot reload for the client. Server + client
# share one port. Signaling rides at /signaling.
dev:
    cd tantrix-app && dx serve --port {{ port }} --hot-reload

# Build + serve the release fullstack binary (no hot reload, no browser).
run:
    cd tantrix-app && dx run --release --port {{ port }} --hot-reload false --open false

# Build the fullstack app and run browser end-to-end tests against it.
e2e:
    cd e2e && npm test

# Expose the fullstack server via an ngrok tunnel. Signaling rides on the
# same tunnel because the client and the signaling server share the port.
# Ctrl+C stops both the server and the tunnel.
tunnel:
    #!/usr/bin/env bash
    set -e
    trap 'kill 0' SIGINT SIGTERM EXIT
    cd tantrix-app && dx run --release --addr 0.0.0.0 --port {{ port }} --open false &
    # Wait for the server to bind before opening the tunnel.
    for i in {1..60}; do
        if curl -sf "http://localhost:{{ port }}" > /dev/null 2>&1; then break; fi
        sleep 1
    done
    ngrok http {{ port }}
