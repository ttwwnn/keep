#!/usr/bin/env bash
# Run the Windows app's interface on this machine (it runs on macOS too, for
# development) against a daemon of its own — never the one that holds your
# real tabs. Attaching to those would resize them to this window: a tab fits
# the smallest viewer looking at it.
#
#   apps/windows/tools/dev-isolado.sh            # vite + the debug app
#   KEEP_SOCKET=/tmp/outro.sock apps/windows/tools/dev-isolado.sh
#
# Stop with Ctrl-C: the app, vite and this run's daemon are ended by pid.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"

export KEEP_SOCKET="${KEEP_SOCKET:-/tmp/keep-dev-$$.sock}"
export KEEP_STATE_DIR="${KEEP_STATE_DIR:-$(mktemp -d)}"
export KEEPD_BIN="${KEEPD_BIN:-$root/target/debug/keepd}"
export KEEP_AI_USAGE_HOME=/var/empty

(cd "$root" && cargo build -p keepd)
(cd "$here/src-tauri" && cargo build)

cd "$here"
npx vite --port 1420 --strictPort &
vite=$!
cleanup() {
    kill "$vite" 2>/dev/null || true
    [ -n "${app:-}" ] && kill "$app" 2>/dev/null || true
    # This run's daemon only: the one listening on this run's socket.
    pid=$(lsof -t "$KEEP_SOCKET" 2>/dev/null | head -1 || true)
    [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
    rm -f "$KEEP_SOCKET"
}
trap cleanup EXIT
for _ in $(seq 1 60); do curl -s -o /dev/null http://localhost:1420 && break; sleep 0.5; done
echo "socket: $KEEP_SOCKET  estado: $KEEP_STATE_DIR"
./src-tauri/target/debug/keep-app &
app=$!
wait "$app"
