#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
port=${1:-8080}
health="http://127.0.0.1:$port/health"
log=$(mktemp)
tunnel=
server=

cleanup() {
    for pid in "$server" "$tunnel"; do
        if [[ -n "$pid" ]]; then
            kill "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    done
    rm -f "$log"
}
fail() {
    echo "error: $*" >&2
    cat "$log" >&2
    exit 1
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

curl -fsS "$health" >/dev/null 2>&1 && fail "port $port is already in use"

cloudflared tunnel --url "http://127.0.0.1:$port" --no-autoupdate >"$log" 2>&1 &
tunnel=$!
origin=
for ((i = 0; i < 60; i++)); do
    origin=$(grep -om1 -E 'https://[a-z0-9-]+\.trycloudflare\.com' "$log" || true)
    [[ -n "$origin" ]] && break
    kill -0 "$tunnel" 2>/dev/null || fail "Cloudflare Tunnel stopped"
    sleep 1
done
[[ -n "$origin" ]] || fail "Cloudflare Tunnel URL timed out"

(cd tantrix-app && TANTRIX_PUBLIC_ORIGIN="$origin" exec dx run --web --features fullstack --release --addr 127.0.0.1 --port "$port" --open false) &
server=$!
for ((i = 0; i < 300; i++)); do
    if curl -fsS "$health" >/dev/null 2>&1; then
        kill -0 "$tunnel" 2>/dev/null || fail "Cloudflare Tunnel stopped"
        echo "Cloudflare Tunnel: $origin"
        wait "$tunnel"
        exit
    fi
    kill -0 "$server" 2>/dev/null || fail "native server stopped"
    sleep 1
done
fail "native server readiness timed out"
