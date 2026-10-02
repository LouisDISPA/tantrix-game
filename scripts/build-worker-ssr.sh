#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

version=$(grep -A1 '^name = "worker"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p')
cargo install worker-build --version "$version" --locked --root target/tools --quiet

# Build the Wasm that worker-build packages.
(cd tantrix-worker && WASM_BINDGEN_USE_JS_SYS=1 cargo build --lib --profile wasm-release --target wasm32-unknown-unknown)

wasm=target/wasm32-unknown-unknown/wasm-release/tantrix_worker.wasm
out=target/dx/tantrix-worker-assets
client=target/dx/tantrix-app/release/web/public/assets
# worker-build recopies this cached Wasm.
cached_wasm=
while IFS= read -r -d '' candidate; do
    if cmp -s "$candidate" "$wasm"; then
        cached_wasm=$candidate
        break
    fi
done < <(find target/wasm32-unknown-unknown/wasm-release -mindepth 2 -type f \
    -name tantrix_worker.wasm -print0)
if [ -z "$cached_wasm" ]; then
    echo "error: no cached Worker Wasm matches $wasm" >&2
    exit 1
fi
rm -rf "$out"
dx tools assets "$cached_wasm" "$out"

shopt -s nullglob
assets=("$out"/*)
if ((${#assets[@]} == 0)); then
    echo "error: no Worker assets found" >&2
    exit 1
fi
for asset in "${assets[@]}"; do
    if [ ! -e "$client/$(basename "$asset")" ]; then
        echo "error: $(basename "$asset") is missing from the client build" >&2
        exit 1
    fi
done

(cd tantrix-worker && ../target/tools/bin/worker-build --profile wasm-release --out-dir ../cloudflare/build/ssr)
