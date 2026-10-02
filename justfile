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
    cd tantrix-app && dx serve --port {{ port }} --web --features fullstack --hot-reload true

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

[group('Worker')]
worker-client:
    rm -rf {{ client_public }}
    cd tantrix-app && dx build --web --release --features fullstack
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
