#!/usr/bin/env bash
set -euo pipefail

APP_PATH=${1:-}
EXPECTED_IDENTIFIER=${SPROUT_DEV_BUNDLE_IDENTIFIER:-xyz.block.sprout.app.dev}
EXPECTED_NAME=${SPROUT_DEV_PRODUCT_NAME:-Sprout Dev}
EXPECTED_SCHEME=${SPROUT_DEV_URL_SCHEME:-sprout-dev}

if [[ -z "$APP_PATH" ]]; then
    echo "usage: $0 /path/to/Sprout Dev.app" >&2
    exit 1
fi

if [[ ! -d "$APP_PATH" ]]; then
    echo "Error: app bundle not found: $APP_PATH" >&2
    exit 1
fi

INFO_PLIST="$APP_PATH/Contents/Info.plist"
MACOS_DIR="$APP_PATH/Contents/MacOS"

if [[ ! -f "$INFO_PLIST" ]]; then
    echo "Error: missing Info.plist: $INFO_PLIST" >&2
    exit 1
fi

if [[ ! -d "$MACOS_DIR" ]]; then
    echo "Error: missing MacOS directory: $MACOS_DIR" >&2
    exit 1
fi

python3 - "$INFO_PLIST" "$EXPECTED_IDENTIFIER" "$EXPECTED_NAME" "$EXPECTED_SCHEME" <<'PY'
import plistlib
import sys

path, expected_identifier, expected_name, expected_scheme = sys.argv[1:]
with open(path, "rb") as f:
    info = plistlib.load(f)

identifier = info.get("CFBundleIdentifier")
if identifier != expected_identifier:
    raise SystemExit(
        f"Error: CFBundleIdentifier is {identifier!r}; expected {expected_identifier!r}"
    )

names = {
    info.get("CFBundleName"),
    info.get("CFBundleDisplayName"),
    info.get("CFBundleExecutable"),
}
if expected_name not in names:
    raise SystemExit(
        "Error: app is not branded as "
        f"{expected_name!r}; found names {sorted(name for name in names if name)!r}"
    )

schemes = set()
for entry in info.get("CFBundleURLTypes", []):
    schemes.update(entry.get("CFBundleURLSchemes", []))

if expected_scheme not in schemes:
    raise SystemExit(
        f"Error: missing URL scheme {expected_scheme!r}; found {sorted(schemes)!r}"
    )

if "sprout" in schemes:
    raise SystemExit("Error: dev app registers production URL scheme 'sprout'")
PY

required_sidecars=(
    buzz-acp
    buzz-agent
    buzz-dev-mcp
    git-credential-nostr
    buzz
)

missing=()
for bin in "${required_sidecars[@]}"; do
    path="$MACOS_DIR/$bin"
    if [[ ! -f "$path" ]]; then
        missing+=("$bin (missing)")
    elif [[ ! -s "$path" ]]; then
        missing+=("$bin (empty)")
    elif [[ ! -x "$path" ]]; then
        missing+=("$bin (not executable)")
    fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
    echo "Error: invalid Sprout Dev sidecar(s): ${missing[*]}" >&2
    exit 1
fi

echo "Validated Sprout Dev app bundle: $APP_PATH"
