#!/usr/bin/env bash
# Local fuzz smoke: run each target briefly. CI runs the same via ci.yml.
set -euo pipefail
cd "$(dirname "$0")/.."
for t in parse_murl parse_manifest canonical_json; do
    echo "=== $t ==="
    cargo +nightly fuzz run "$t" -- -max_total_time="${1:-30}" 2>&1 | tail -3
done
echo "fuzz smoke: all targets clean"
