#!/usr/bin/env bash
#
# The tabs come back by themselves after a restart (keep restauro, 12/10/2026).
#
# A daemon of its own holds three tabs in two workspaces, each in a folder of
# its own, and its picture is taken (`keep restauro retrato`, what the app does
# every 30 s). Then the "restart": that daemon dies, a new empty one comes up,
# and KeepDev opens on it. Before putting anything on screen the app asks the
# core for the last daemon's tabs (`keep restauro restaurar --fase=abas`):
# the same workspaces and tabs, each in its folder, come back, and the default
# workspace a fresh daemon used to get is not made. In the background the app
# then keeps the picture (`keep restauro manter`): the old one moves to
# anterior.json and the new daemon's is written.
#
#   tools/restauro-test.sh [--sabotage]
#
# --sabotage: an app that skips the restore at launch must fail the checks.
#
# Your app, your daemon and your pictures are never touched: a scratch socket,
# KEEP_STATE_DIR in a scratch folder (the pictures live in its restauro/).

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=$(dev_app)
APP_NAME=$(app_name "$APP")
BIN=$APP/Contents/MacOS/$APP_NAME
WORK=$(mktemp -d /tmp/keep-restauro-XXXXXX)
SOCKET=/tmp/kr-$$.sock
KEEP=$PWD/target/release/keep
PASSED=0
FAILED=0

cleanup() {
    [ -n "${APP_PID:-}" ] && kill -9 "$APP_PID" 2>/dev/null
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
    pkill -f "keepd.*$SOCKET" 2>/dev/null
    [ -n "${KEEP_WORK:-}" ] && printf 'kept: %s\n' "$WORK" || rm -rf "$WORK"
    rm -f "$SOCKET"
}
trap cleanup EXIT INT TERM

say() { printf '%s\n' "$*"; }
check() {  # check <what> <wanted> <got>
    if [ "$3" = "$2" ]; then say "  ok    $1"; PASSED=$((PASSED + 1))
    else say "  FAIL  $1"; say "        wanted: $2"; say "        got:    $3"; FAILED=$((FAILED + 1)); fi
}

export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$WORK/state
export KEEP_RESTAURO_ESTADO=$KEEP_STATE_DIR/restauro
mkdir -p "$KEEP_STATE_DIR" "$WORK/um" "$WORK/dois" "$WORK/tres"
require_scratch_socket

start_daemon() {
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null && sleep 1
    rm -f "$SOCKET"
    ./target/release/keepd >>"$WORK/daemon.log" 2>&1 &
    DAEMON_PID=$!
    local waited=0
    while [ ! -S "$SOCKET" ] && [ "$waited" -lt 40 ]; do sleep 0.25; waited=$((waited + 1)); done
    sleep 0.5
}

# workspace<TAB>folder of every live tab, sorted
tabs() {
    "$KEEP" restauro retrato --print | python3 -c '
import json, os, sys
d = json.load(sys.stdin)
print(";".join(sorted("%s=%s" % (a["workspace"], os.path.basename(a["cwd"].rstrip("/"))) for a in d["abas"])))'
}

say "building $APP_NAME, the client and the daemon (yours is left alone)"
./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }

start_daemon
(cd "$WORK/um" && "$KEEP" new Alfa >/dev/null)
(cd "$WORK/dois" && "$KEEP" new Alfa >/dev/null)
(cd "$WORK/tres" && "$KEEP" new Beta >/dev/null)
sleep 1
ANTES=$(tabs)
check "three tabs before the restart" "Alfa=dois;Alfa=um;Beta=tres" "$ANTES"
"$KEEP" restauro retrato >/dev/null
check "the picture is written" "yes" "$([ -f "$KEEP_RESTAURO_ESTADO/retrato.json" ] && echo yes || echo no)"

say "the restart: the daemon dies, an empty one comes up, the app opens on it"
start_daemon
check "the new daemon is empty" "" "$(tabs)"
SKIP=""
[ "${1:-}" = "--sabotage" ] && SKIP=1
if [ -n "$SKIP" ]; then
    # an app without the restore at launch: its default workspace, nothing else
    "$KEEP" new "$(id -un)" >/dev/null
else
    KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
    APP_PID=$!
fi
waited=0
while [ -z "$(tabs)" ] && [ "$waited" -lt 60 ]; do sleep 0.5; waited=$((waited + 1)); done
sleep 3
check "the same tabs, each in its folder, are back" "$ANTES" "$(tabs)"
check "no default workspace was made on top" "" "$("$KEEP" restauro retrato --print | python3 -c "
import json, sys
print(' '.join(a['workspace'] for a in json.load(sys.stdin)['abas'] if a['workspace'] == '$(id -un)'))")"
waited=0
while [ ! -f "$KEEP_RESTAURO_ESTADO/anterior.json" ] && [ "$waited" -lt 90 ]; do sleep 1; waited=$((waited + 1)); done
check "the app keeps the picture: the old one goes to anterior.json" "yes" \
    "$([ -f "$KEEP_RESTAURO_ESTADO/anterior.json" ] && echo yes || echo no)"
check "the restore in the new daemon is done" "True" "$(python3 -c "
import json; print(json.load(open('$KEEP_RESTAURO_ESTADO/restaurado.json')).get('feito'))" 2>/dev/null)"

say "$PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
