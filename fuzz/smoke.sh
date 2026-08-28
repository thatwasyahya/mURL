#!/usr/bin/env bash
# Local fuzz smoke: run each target briefly. CI runs the same via ci.yml.
#
# On success this prints the last few lines per target, because a passing
# fuzz run is thousands of uninteresting lines. On failure it prints
# everything — an earlier version piped through `tail -3` unconditionally,
# and when CI failed here it threw the diagnostic away and left a stack
# trace with no message above it. Never discard the output of the thing you
# are trying to diagnose.
set -euo pipefail

cd "$(dirname "$0")/.."
SECONDS_PER_TARGET="${1:-30}"
LOG="$(mktemp -d)/fuzz.log"
trap 'rm -rf "$(dirname "$LOG")"' EXIT

status=0
for t in parse_murl parse_manifest canonical_json parse_bundle daemon_protocol; do
    echo "=== $t ==="
    if cargo +nightly fuzz run "$t" -- -max_total_time="$SECONDS_PER_TARGET" > "$LOG" 2>&1; then
        tail -3 "$LOG"
    else
        echo "--- $t FAILED; full output follows ---"
        cat "$LOG"
        status=1
    fi
done

if [ "$status" -eq 0 ]; then
    echo "fuzz smoke: all targets clean"
else
    echo "fuzz smoke: at least one target failed"
fi
exit "$status"
