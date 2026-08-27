#!/usr/bin/env bash
# Build mURL.app — the macOS bundle that lets Launch Services associate the
# murl:// scheme with the CLI.
#
# Why a bundle at all: Launch Services reads CFBundleURLTypes from an
# application bundle's Info.plist at registration time. A bare CLI binary
# has no Info.plist and therefore cannot claim a scheme, no matter what it
# writes to disk. The bundle is scaffolding around the same `murl` binary —
# it adds no logic, and every security property comes from the binary it
# wraps.
#
# Usage:
#   packaging/macos/build-app.sh [--release] [--out DIR]
#   Then: open the bundle once (or `lsregister -f mURL.app`) to register.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PROFILE="debug"
OUT_DIR="$ROOT/target/macos"

while [ $# -gt 0 ]; do
    case "$1" in
        --release) PROFILE="release"; shift ;;
        --out) OUT_DIR="$2"; shift 2 ;;
        -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

if [ "$(uname -s)" != "Darwin" ]; then
    echo "note: building the bundle layout on $(uname -s); registration requires macOS." >&2
fi

echo "building murl ($PROFILE)..."
if [ "$PROFILE" = "release" ]; then
    (cd "$ROOT" && cargo build --release -p murl-cli)
else
    (cd "$ROOT" && cargo build -p murl-cli)
fi

BIN="$ROOT/target/$PROFILE/murl"
[ -x "$BIN" ] || { echo "error: $BIN not found" >&2; exit 1; }

APP="$OUT_DIR/mURL.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -1)"
sed "s/@VERSION@/$VERSION/g" "$ROOT/packaging/macos/Info.plist.in" \
    > "$APP/Contents/Info.plist"

# The real binary ships inside the bundle; the launcher stub execs it so
# there is exactly one copy of the logic.
cp "$BIN" "$APP/Contents/MacOS/murl"
cat > "$APP/Contents/MacOS/mURL" <<'LAUNCHER'
#!/bin/sh
# Launch Services hands the activated URL as "$1". Pass it to `murl open`
# unchanged: the parser re-validates it from scratch, so nothing here needs
# to interpret, quote, or sanitize it.
exec "$(dirname "$0")/murl" open "$@"
LAUNCHER
chmod +x "$APP/Contents/MacOS/mURL" "$APP/Contents/MacOS/murl"

printf 'APPL????' > "$APP/Contents/PkgInfo"

echo "built $APP (murl $VERSION)"
echo
echo "register it:"
echo "  open '$APP'    # or:"
echo "  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f '$APP'"
echo
echo "then test:  open 'murl://local/<name>'"
