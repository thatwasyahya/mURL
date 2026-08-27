#!/usr/bin/env bash
# Demonstrate the daemon path end to end, hermetically.
#
# Shows: socket creation with user-only permissions, the wire protocol,
# activation through the daemon, the fail-closed behavior when no terminal
# is attached to answer consent, and the CLI's silent fallback once the
# daemon is gone. Nothing outside the temp directory is touched and nothing
# real is launched (the opener is /bin/true).

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MURL="${MURL_BIN:-$ROOT/target/debug/murl}"
DAEMON="${MURL_DAEMON_BIN:-$ROOT/target/debug/murl-daemon}"

if [ ! -x "$MURL" ] || [ ! -x "$DAEMON" ]; then
    echo "building..."
    (cd "$ROOT" && cargo build -q)
fi

TMP="$(mktemp -d)"
DAEMON_PID=""
cleanup() {
    [ -n "$DAEMON_PID" ] && kill "$DAEMON_PID" 2>/dev/null || true
    rm -rf "$TMP"
}
trap cleanup EXIT

export MURL_CONFIG_DIR="$TMP/config"
export MURL_DATA_DIR="$TMP/data"
export MURL_CACHE_DIR="$TMP/cache"
export MURL_SOCKET="$TMP/run/murl.sock"

step() { printf '\n\033[1m== %s ==\033[0m\n' "$*"; }

mkdir -p "$MURL_CONFIG_DIR"
# Point dispatch at a no-op so the demo launches nothing real.
printf '{"open":["/bin/true"]}\n' > "$MURL_CONFIG_DIR/handlers.json"

cat > "$TMP/demo.murl.json" <<'JSON'
{
  "murlVersion": "0.2",
  "id": "murl://local/daemon-demo",
  "name": "Daemon Demo",
  "resources": [
    {"id": "docs", "kind": "https", "target": "https://docs.example/x", "role": "docs"},
    {"id": "site", "kind": "https", "target": "https://site.example/x"}
  ]
}
JSON

step "1. Install a destination"
"$MURL" name add daemon-demo "$TMP/demo.murl.json"

step "2. Start the daemon"
"$DAEMON" run 2>"$TMP/daemon.log" &
DAEMON_PID=$!
# Wait for the socket rather than sleeping blindly.
for _ in $(seq 1 50); do
    [ -S "$MURL_SOCKET" ] && break
    sleep 0.1
done
[ -S "$MURL_SOCKET" ] || { echo "daemon did not start:"; cat "$TMP/daemon.log"; exit 1; }

step "3. Socket is user-private (0600 inside a 0700 directory)"
ls -ld "$TMP/run"
ls -l "$MURL_SOCKET"

step "4. Daemon status"
"$DAEMON" status

step "5. Resolve through the daemon — a plan, and nothing opened"
"$MURL" --daemon --json resolve "murl://local/daemon-demo" >/dev/null 2>&1 || true
printf '{"type":"resolve","protocol":1,"murl":"murl://local/daemon-demo"}\n' \
    | timeout 5 socat - "UNIX-CONNECT:$MURL_SOCKET" 2>/dev/null \
    | head -c 200 || echo "(install socat to see the raw protocol)"
echo

step "6. Activate with no terminal attached — consent fails closed"
set +e
"$MURL" --daemon open "murl://local/daemon-demo" < /dev/null
echo "exit code: $?  (4 = DENIED, which is correct without a way to ask)"
set -e

step "7. Protocol violations are refused"
printf '{"type":"launch","protocol":1}\n{"type":"ping","protocol":99}\n' \
    | timeout 5 socat - "UNIX-CONNECT:$MURL_SOCKET" 2>/dev/null \
    || echo "(socat unavailable)"

step "8. Stop the daemon; the CLI falls back silently"
kill "$DAEMON_PID"; DAEMON_PID=""
sleep 0.3
"$MURL" open "murl://local/daemon-demo" --dry-run | tail -5

step "9. --daemon makes the daemon mandatory instead of optional"
set +e
"$MURL" --daemon open "murl://local/daemon-demo" 2>&1 | tail -2
set -e

printf '\n\033[1mDaemon demo complete.\033[0m State was confined to %s (now removed).\n' "$TMP"
