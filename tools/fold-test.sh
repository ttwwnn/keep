#!/usr/bin/env bash
#
# A workspace folded shut in the sidebar stays folded.
#
# The window reports the sidebar's width on every relayout of its split view,
# and coming back from another app is one. That report used to be saved whole,
# and a measured state knows nothing of folds or vertical tabs: every folded
# workspace opened again the moment you switched apps and came back.
#
# The sidebar state is seeded with one workspace folded and vertical tabs on;
# the app is sent behind another one and brought back, then its window is
# resized, and the saved state must still say what was seeded.
#
#   tools/fold-test.sh
#
# Your app and your daemon are never touched.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=$(dev_app)
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
SOCKET=/tmp/keep-fold-$$.sock
WORK=$(mktemp -d /tmp/keep-fold-XXXXXX)
FRONT=$(osascript -e 'id of application (path to frontmost application as text)' 2>/dev/null | tr -d '\000')
PASSED=0
FAILED=0

cleanup() {
    [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
    [ -n "$FRONT" ] && osascript -e "tell application id \"$FRONT\" to activate" >/dev/null 2>&1
    [ -n "${KEEP_WORK:-}" ] && say "kept: $WORK" || rm -rf "$WORK"
    rm -f "$SOCKET"
}
trap cleanup EXIT INT TERM

say() { printf '%s\n' "$*"; }
check() {  # check <what> <wanted> <got>
    if [ "$3" = "$2" ]; then
        say "  ok    $1"
        PASSED=$((PASSED + 1))
    else
        say "  FAIL  $1"
        say "        wanted: $2"
        say "        got:    $3"
        FAILED=$((FAILED + 1))
    fi
}

# What the saved state says, in one line: folded workspaces, vertical tabs,
# collapsed, width.
saved() {
    python3 -c '
import json, sys
s = json.load(open(sys.argv[1]))["0"]
print(",".join(sorted(s.get("folded", []))), s.get("verticalTabs", False),
      s.get("isCollapsed"), int(s.get("width", 0)))
' "$KEEP_STATE_DIR/sidebar-state.json" 2>/dev/null || echo "(unreadable)"
}

# Long enough for the app's 0.3 s divider report and the store's write.
settle() { sleep 2; }

say "building $APP_NAME, the client, the daemon and the tools (yours is left alone)"
./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }
AXPRESS=$WORK/axpress
swiftc -O tools/axpress.swift -o "$AXPRESS" 2>/dev/null || { say "could not build axpress"; exit 1; }

export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$WORK/state
mkdir -p "$KEEP_STATE_DIR"
require_scratch_socket
rm -f "$SOCKET"

./target/release/keepd >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
sleep 1
./target/release/keep new um >/dev/null 2>&1 || { say "could not make a workspace"; exit 1; }
./target/release/keep new dois >/dev/null 2>&1 || { say "could not make a workspace"; exit 1; }
cat >"$KEEP_STATE_DIR/windows.json" <<'JSON'
{"0":{"x":360,"y":160,"width":1100,"height":700,"workspaces":["um","dois"],"tab":{"workspace":"um","root":1}}}
JSON
cat >"$KEEP_STATE_DIR/sidebar-state.json" <<'JSON'
{"0":{"folded":["dois"],"isCollapsed":false,"verticalTabs":true,"width":260}}
JSON

stop_app
"$BIN" >"$WORK/app.log" 2>&1 &
APP_PID=$!
sleep 5
check "the app is up" "yes" "$(kill -0 "$APP_PID" 2>/dev/null && echo yes || echo no)"
settle
check "folds survive the launch" "dois True False 260" "$(saved)"

osascript -e 'tell application "Finder" to activate' >/dev/null 2>&1
settle
osascript -e "tell application \"System Events\" to set frontmost of process \"$APP_NAME\" to true" \
    >/dev/null 2>&1
settle
check "folds survive another app and back" "dois True False 260" "$(saved)"

# A resize of the window lays the split view out again, which is what an app
# switch did; the divider report it makes must not carry the folds away.
osascript -e "tell application \"System Events\" to tell process \"$APP_NAME\" to set size of window 1 to {980, 640}" \
    >/dev/null 2>&1
settle
check "folds survive a resize" "dois True False 260" "$(saved)"

# The sidebar's own toggle still reaches the saved state, and takes the folds
# along rather than leaving them behind.
"$AXPRESS" "$APP_NAME" menu View "Toggle Sidebar" >/dev/null 2>&1
settle
check "the sidebar collapses, folds kept" "dois True True 260" "$(saved)"
"$AXPRESS" "$APP_NAME" menu View "Toggle Sidebar" >/dev/null 2>&1
settle
check "the sidebar opens again at its width, folds kept" "dois True False 260" "$(saved)"

stop_app
APP_PID=
say "$PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
