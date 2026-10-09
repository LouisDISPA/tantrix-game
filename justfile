# Tantrix commands. See README.md for setup.

port := "8080"

default:
    @just --list

[group('Checks')]
fmt-check:
    cargo fmt --all --check

[group('Checks')]
lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p tantrix-app --features server --all-targets -- -D warnings
    cargo clippy -p tantrix-app --features web --target wasm32-unknown-unknown -- -D warnings

[group('Checks')]
test:
    cargo test --workspace
    cargo test -p tantrix-app --features server

[group('Local')]
dev:
    cd tantrix-app && dx serve --port {{ port }} --web --features fullstack,devtools --hot-reload true

[group('Local')]
run:
    cd tantrix-app && dx run --web --features fullstack --release --port {{ port }} --open false

[group('Setup')]
setup-e2e:
    cd e2e && pnpm install --frozen-lockfile

[group('Setup')]
setup-worker:
    cd cloudflare && pnpm install --frozen-lockfile

[group('Browser tests')]
native-e2e: setup-e2e
    cd e2e && pnpm run test:native

[group('Local')]
tunnel:
    ./scripts/tunnel.sh {{ port }}

client_public := "target/dx/tantrix-app/release/web/public"

# The browser client is built with a std rebuilt from source (nightly build-std,
# no panic machinery) and without panic locations: 1.69 MB -> 0.98 MB of wasm.
# A panic aborts without a message. `@client` scopes build-std to the browser
# build, because dx's host server build cannot use it, and the rustflags
# variable only reaches wasm32 builds. The Worker SSR module (`worker-ssr`)
# keeps the stock toolchain. Do not add `-Zfmt-debug=none`: `Color`'s `Display`
# and several `key:` / `data-*` attributes go through `Debug`.
client_std := "--cargo-args=-Zbuild-std=std,panic_abort -Zbuild-std-features="
client_flags := "-Zlocation-detail=none -Zunstable-options -Cpanic=immediate-abort"

[group('Worker')]
worker-client:
    rm -rf {{ client_public }}
    cd tantrix-app && CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="{{ client_flags }}" dx build @client --web --release --features fullstack "{{ client_std }}" @server --server --release --features fullstack
    printf 'index.html\n' > {{ client_public }}/.assetsignore

[private]
worker-ssr: worker-client
    ./scripts/build-worker-ssr.sh

[group('Worker')]
worker-build: worker-ssr

[group('Worker')]
worker-dev: worker-build setup-worker
    cd cloudflare && pnpm exec wrangler dev

[group('Browser tests')]
worker-e2e: worker-build setup-worker setup-e2e
    cd e2e && pnpm run test:worker

[group('Worker')]
worker-deploy: worker-build setup-worker
    cd cloudflare && pnpm exec wrangler deploy
