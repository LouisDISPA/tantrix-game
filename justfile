# Tantrix dev commands. Run `just` to list available recipes.
#
# Prerequisites:
#   cargo install just              (task runner)
#   cargo install dioxus-cli        (dx)
#   ngrok                           (for the tunnel recipe)
#   Node.js/npm                     (for the worker-* recipes; they install
#                                    worker-build and wrangler on first use)
#
# The native Dioxus fullstack binary and Cloudflare Worker both host online rooms.

port := "8080"

default:
    @just --list

# ── Rust checks (CI runs these same recipes) ────────────────────────────────

# Fail on unformatted Rust code.
fmt-check:
    cargo fmt --all --check

# Clippy, warnings denied, for the native workspace, native server and wasm client.
lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p tantrix-app --features server --all-targets -- -D warnings
    cargo clippy -p tantrix-app --features web --target wasm32-unknown-unknown -- -D warnings

# Unit and integration tests, including the native server's.
test:
    cargo test --workspace
    cargo test -p tantrix-app --features server

# Fullstack UI and online rooms with hot reload.
dev:
    cd tantrix-app && dx serve --port {{ port }} --web --features fullstack --hot-reload true

# Build + serve the release fullstack binary (no hot reload, no browser).
run:
    cd tantrix-app && dx run --release --port {{ port }} --hot-reload false --open false

# Build the Worker and run the browser end-to-end suite against it.
e2e: worker-e2e

# Build the native fullstack app and run the browser end-to-end suite against it.
native-e2e:
    cd e2e && if [ ! -d node_modules ]; then npm ci; fi && npm run test:native

# Expose the native UI development server via an ngrok tunnel.
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

# ── Cloudflare Worker: server-side rendering + hydration ────────────────────
# Production runs on Cloudflare (cloudflare/): the Worker server-renders every
# page with the Rust `tantrix-worker` module, static files come from the
# Workers assets binding, and room WebSockets use the `Room` Durable Object.

client_public := "target/dx/tantrix-app/release/web/public"

# Release web client that hydrates server-rendered pages.
worker-client:
    # dx never deletes superseded hashed bundles: start clean so only this
    # build's files are uploaded and checked against the SSR module.
    rm -rf {{ client_public }}
    cd tantrix-app && dx build --web --release --features fullstack
    # The Worker renders every page, so the static index.html is not uploaded.
    printf 'index.html\n' > {{ client_public }}/.assetsignore

# SSR module for the Worker. Run after `worker-client`: it embeds the client's
# index.html and must link the same hashed CSS files.
worker-ssr:
    #!/usr/bin/env bash
    set -euo pipefail
    # worker-build must match the `worker` crate: install that exact version
    # into target/tools (a no-op once it is there).
    version=$(grep -A1 '^name = "worker"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p')
    cargo install worker-build --version "$version" --locked --root target/tools --quiet
    worker_build="$PWD/target/tools/bin/worker-build"
    # Same cargo invocation as worker-build's, so its own build step below is a
    # no-op and keeps the asset paths patched in by `dx tools assets`. The
    # size-optimized `wasm-release` profile is defined in the root Cargo.toml.
    (cd tantrix-worker && WASM_BINDGEN_USE_JS_SYS=1 cargo build --lib --profile wasm-release --target wasm32-unknown-unknown)
    # Plain cargo builds point `asset!()` at the source files on disk; this
    # rewrites them to the hashed names dx gave the client's copies. Patch the
    # deps/ artifact: even a no-op cargo build re-copies it over wasm-release/.
    out=target/dx/tantrix-worker-assets
    rm -rf "$out"
    dx tools assets target/wasm32-unknown-unknown/wasm-release/deps/tantrix_worker.wasm "$out"
    for f in "$out"/*; do
        if [ ! -e "{{ client_public }}/assets/$(basename "$f")" ]; then
            echo "error: $(basename "$f") is not in the client build; run just worker-client" >&2
            exit 1
        fi
    done
    (cd tantrix-worker && "$worker_build" --profile wasm-release --out-dir ../cloudflare/build/ssr)

# Build the client and the SSR module.
worker-build: worker-client worker-ssr

# Install wrangler, pinned by cloudflare/package-lock.json, on first use.
_wrangler:
    cd cloudflare && if [ ! -d node_modules ]; then npm ci; fi

# Run the production Worker locally (http://localhost:8787).
worker-dev: worker-build _wrangler
    cd cloudflare && npx wrangler dev

# Run the e2e suite against the Worker in wrangler dev.
worker-e2e: worker-build _wrangler
    cd e2e && if [ ! -d node_modules ]; then npm ci; fi && npm run test:worker

# Build and deploy the Worker.
worker-deploy: worker-build _wrangler
    cd cloudflare && npx wrangler deploy
