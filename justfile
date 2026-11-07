# Tantrix commands. See README.md for setup.

port := "8080"

default:
    @just --list

[group('Checks')]
fmt-check:
    cargo fmt --all --check

[group('Checks')]
test-e2e: setup-e2e
    cd e2e && pnpm run test:native

# Desktop checks need webkit2gtk and GTK development packages on Linux. The
# Worker compiles only for wasm32 and embeds the client's index.html, so its
# lint uses a stand-in page instead of building the client.

[group('Checks')]
lint:
    cargo clippy --locked --workspace --all-targets -- -D warnings
    cargo clippy --locked -p tantrix-app --features server --all-targets -- -D warnings
    cargo clippy --locked -p tantrix-app --features desktop,bundle --all-targets -- -D warnings
    cargo clippy --locked -p tantrix-app --features web --target wasm32-unknown-unknown -- -D warnings
    TANTRIX_INDEX_HTML="{{ justfile_directory() }}/tantrix-worker/stub-index.html" cargo clippy --locked -p tantrix-worker --target wasm32-unknown-unknown -- -D warnings

[group('Checks')]
test:
    cargo test --locked --workspace
    cargo test --locked -p tantrix-app --features server
    cargo test --locked -p tantrix-app --features desktop,bundle

# Everything CI runs, in order.
[group('Checks')]
ci: fmt-check lint test test-e2e

[group('Local')]
dev:
    cd tantrix-app && dx serve --port {{ port }} --web --fullstack true --features fullstack,devtools --hot-reload true

[group('Local')]
run:
    cd tantrix-app && dx run --web --fullstack true --features fullstack --release --port {{ port }} --open false

[group('Local')]
fmt:
    cargo fmt --all
    cd tantrix-app && dx fmt

[group('Local')]
desktop:
    cd tantrix-app && dx serve --desktop --features desktop,bundle

[group('Local')]
desktop-bundle-macos:
    cd tantrix-app && dx bundle --release --macos --package tantrix-app --features desktop,bundle --package-types macos

[group('Setup')]
setup-e2e:
    cd e2e && pnpm install --frozen-lockfile

[group('Setup')]
setup-worker:
    cd cloudflare && pnpm install --frozen-lockfile

[group('Local')]
tunnel:
    ./scripts/tunnel.sh {{ port }}

client_public := "target/dx/tantrix/release/web/public"

# The browser client is built with a std rebuilt from source (nightly build-std,
# no panic machinery) and without panic locations: 1.69 MB -> 0.98 MB of wasm.
# A panic aborts without a message. `@client` scopes build-std to the browser
# build, because dx's host server build cannot use it, and the rustflags
# variable only reaches wasm32 builds. The Worker SSR module (`worker-ssr`)
# keeps the stock toolchain. Do not add `-Zfmt-debug=none`: `Color`'s `Display`
# and several `key:` / `data-*` attributes go through `Debug`.
client_std := "--cargo-args=-Zbuild-std=std,panic_abort -Zbuild-std-features="
client_flags := "-Zlocation-detail=none -Zunstable-options -Cpanic=immediate-abort"

[private]
worker-client-build:
    rm -rf {{ client_public }}
    cd tantrix-app && CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="{{ client_flags }}" dx build @client --web --release --features fullstack "{{ client_std }}" @server --server --release --features fullstack
    printf 'index.html\n' > {{ client_public }}/.assetsignore

[private]
worker-ssr-build: worker-client-build
    ./scripts/build-worker-ssr.sh

[group('Worker')]
worker-e2e: worker-ssr-build setup-worker setup-e2e
    cd e2e && pnpm run test:worker

[group('Worker')]
worker-deploy: worker-ssr-build setup-worker
    cd cloudflare && pnpm exec wrangler deploy

# vendor/dioxus-server is a published dioxus-server release plus its
# upstream.diff; see its PATCHES.md. The release is the version in its
# Cargo.toml unless `vendor-update` names a new one.

# Fail unless vendor/dioxus-server equals its release plus upstream.diff.
[group('Vendor')]
vendor-check: (_dioxus-server "check")

# Rewrite upstream.diff from the edited vendor/dioxus-server.
[group('Vendor')]
vendor-diff: (_dioxus-server "diff")

# Replace vendor/dioxus-server with another release and reapply upstream.diff.
[group('Vendor')]
vendor-update version: (_dioxus-server "update" version)

_dioxus-server mode version="":
    #!/usr/bin/env bash
    set -euo pipefail
    dir=vendor/dioxus-server
    version="{{ version }}"
    [ -n "$version" ] || version=$(awk -F'"' '/^version = /{print $2; exit}' "$dir/Cargo.toml")
    # Personal git settings would change how the diff is written and applied.
    plain_git() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git "$@"; }
    if [ "{{ mode }}" = update ] && [ -n "$(git status --porcelain -- "$dir")" ]; then
        echo "Commit or discard the changes in $dir first: the update replaces it." >&2
        exit 1
    fi
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    # a/ is the release without packaging files: the lockfile is unused and
    # Cargo.toml.orig inherits from the Dioxus workspace.
    curl -sSfL "https://static.crates.io/crates/dioxus-server/dioxus-server-$version.crate" | tar -xzf - -C "$tmp"
    mv "$tmp/dioxus-server-$version" "$tmp/a"
    rm -f "$tmp/a/Cargo.lock" "$tmp/a/Cargo.toml.orig" "$tmp/a/.cargo_vcs_info.json"
    if [ "{{ mode }}" = update ]; then
        find "$dir" -mindepth 1 -maxdepth 1 ! -name PATCHES.md ! -name upstream.diff -exec rm -rf {} +
        cp -R "$tmp/a/." "$dir/"
        if ! plain_git apply --reject --directory="$dir" "$dir/upstream.diff"; then
            echo "Redo the hunks in each *.rej file, and for each file git reported missing, then delete the .rej files." >&2
            echo "upstream.diff keeps the old hunks until just vendor-diff rewrites it." >&2
            exit 1
        fi
        echo "$dir is dioxus-server $version plus upstream.diff; build it, then run just vendor-diff."
        exit 0
    fi
    # b/ is the vendored copy as git sees it (minus files deleted from disk),
    # without PATCHES.md and the diff.
    mkdir "$tmp/b"
    git -C "$dir" ls-files -z --cached --others --exclude-standard |
        while IFS= read -r -d '' f; do if [ -e "$dir/$f" ]; then printf '%s\0' "$f"; fi; done |
        tar -C "$dir" --null -T - -cf - | tar -xf - -C "$tmp/b"
    rm -f "$tmp/b/PATCHES.md" "$tmp/b/upstream.diff"
    if [ "{{ mode }}" = diff ]; then
        plain_git -C "$tmp" diff --no-index --no-prefix a b > "$dir/upstream.diff" || [ $? = 1 ]
        echo "Wrote $dir/upstream.diff against dioxus-server $version; update PATCHES.md to match."
        exit 0
    fi
    if ! plain_git -C "$tmp/a" apply "$PWD/$dir/upstream.diff"; then
        echo "upstream.diff does not apply to dioxus-server $version; see the upgrade steps in PATCHES.md." >&2
        exit 1
    fi
    if ! plain_git -C "$tmp" diff --no-index --no-prefix a b >&2; then
        echo "$dir is not dioxus-server $version plus upstream.diff (diff above: a/ expected, b/ actual)." >&2
        echo "If the change is intended, run just vendor-diff and update PATCHES.md." >&2
        exit 1
    fi
    echo "$dir is dioxus-server $version plus upstream.diff."
