#!/usr/bin/env bash
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
HOST_TARGET=$(rustc -vV | sed -n 's|host: ||p')
TARGET=${1:-$HOST_TARGET}

cd "$ROOT"

run_pnpm() {
    if command -v pnpm >/dev/null 2>&1; then
        pnpm "$@"
    elif command -v corepack >/dev/null 2>&1; then
        corepack pnpm "$@"
    else
        echo "Error: pnpm or corepack is required" >&2
        exit 1
    fi
}

if [[ "$TARGET" != "$HOST_TARGET" ]]; then
    echo "Error: cross-target dev app builds are not supported by this script." >&2
    echo "Host target: $HOST_TARGET" >&2
    echo "Requested target: $TARGET" >&2
    exit 1
fi

cargo build --release \
    -p sprout-acp \
    -p sprout-agent \
    -p sprout-dev-mcp \
    -p git-credential-nostr \
    -p sprout-cli

./scripts/bundle-sidecars.sh "$TARGET"

run_pnpm install

cd desktop
run_pnpm tauri build \
    --features mesh-llm \
    --target "$TARGET" \
    --config src-tauri/tauri.dev.conf.json \
    --config '{"build":{"beforeBuildCommand":"corepack pnpm build"}}' \
    --bundles app \
    --no-sign \
    --ci

APP="$ROOT/desktop/src-tauri/target/$TARGET/release/bundle/macos/Sprout Dev.app"
if [[ ! -d "$APP" ]]; then
    APP="$ROOT/desktop/src-tauri/target/release/bundle/macos/Sprout Dev.app"
fi

cd "$ROOT"
./scripts/validate-dev-app-bundle.sh "$APP"

echo "Built clean Sprout Dev app: $APP"
