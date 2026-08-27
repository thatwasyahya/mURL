#!/usr/bin/env bash
# End-to-end mURL demonstration.
#
# Hermetic: state lives in a temp directory (MURL_*_DIR overrides) and the
# only `open` is a --dry-run, so nothing is launched and nothing on your
# machine is modified.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${MURL_BIN:-$ROOT/target/debug/murl}"

if [ ! -x "$BIN" ]; then
    echo "building murl..."
    (cd "$ROOT" && cargo build -q)
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
export MURL_CONFIG_DIR="$TMP/config"
export MURL_DATA_DIR="$TMP/data"
export MURL_CACHE_DIR="$TMP/cache"

step() { printf '\n\033[1m== %s ==\033[0m\n' "$*"; }
run()  { printf '\n$ murl %s\n' "$*"; "$BIN" "$@"; }

step "1. Parse an mURL into its components"
run parse "murl://local/demo/project-x#monitoring"

step "2. Validate the example manifests"
run validate "$ROOT/examples/team.murl.json"
run validate "$ROOT/examples/project-x.murl.json"

step "3. Install them under local names"
run name add demo/team "$ROOT/examples/team.murl.json"
run name add demo/project-x "$ROOT/examples/project-x.murl.json"
run name list

step "4. Resolve the full dispatch plan (note the nested murl://local/demo/team splice)"
run resolve "murl://local/demo/project-x"

step "5. Selector: address one resource of the destination"
run resolve "murl://local/demo/project-x#monitoring"

step "6. Dry-run activation (nothing is launched)"
run open "murl://local/demo/project-x" --dry-run

step "7. Sign a manifest and verify it"
cp "$ROOT/examples/project-x.murl.json" "$TMP/signed.murl.json"
run keygen
run sign "$TMP/signed.murl.json"
run verify "$TMP/signed.murl.json"

step "8. Machine-readable resolution (--json)"
printf '\n$ murl --json resolve murl://local/demo/project-x | head -n 30\n'
"$BIN" --json resolve "murl://local/demo/project-x" | head -n 30

printf '\n\033[1mDemo complete.\033[0m All state was confined to %s (now removed).\n' "$TMP"
