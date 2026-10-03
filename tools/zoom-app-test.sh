#!/usr/bin/env bash
#
# The zoom above the sidebar (UI/ZoomControl.swift), in the app: its buttons,
# its chords and its menu items change the size of every tab's text, step by
# Chrome's steps; it stops at 50% and at 300% without the key falling through
# to a pane; it goes with a shut sidebar, loses its percentage in a narrow
# one, and opens next time at the size it was left at.
#
# What is measured is what the zoom is for: the size the app writes down
# (terminal.json, in a state directory of the test's own) and the columns and
# rows the daemon was given for the tab — a larger text is a smaller grid.
#
#   tools/zoom-app-test.sh            (SKIP_BUILD=1 to use the KeepDev built last;
#                                      KEEP_TEST_APP=<path to an .app> to drive another build)
#
# No mouse: the buttons and menu items are pressed through the accessibility
# tree (tools/axpress.swift), the chords posted to the app's pid alone
# (tools/sendkey.swift), and the front is handed back to whatever had it, so
# the test can run behind the app you are working in. Your app, its daemon
# and its settings are never touched.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=${KEEP_TEST_APP:-$(dev_app)}
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
SOCKET=/tmp/keep-zoom-$$.sock
WORK=$(mktemp -d /tmp/keep-zoom-app-XXXXXX)
AXPRESS=$WORK/axpress
SENDKEY=$WORK/sendkey
STATE=$WORK/state
PASSED=0
FAILED=0

cleanup() {
    [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
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

if [ -z "${KEEP_TEST_APP:-}" ] && [ -z "${SKIP_BUILD:-}" ]; then
    say "building $APP_NAME (yours is left alone)"
    ./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }
fi
swiftc -O tools/axpress.swift -o "$AXPRESS" 2>/dev/null || { say "could not build axpress"; exit 1; }
swiftc -O tools/sendkey.swift -o "$SENDKEY" 2>/dev/null || { say "could not build sendkey"; exit 1; }

export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$STATE
export KEEP_WORKTREES_BIN=/var/empty
mkdir -p "$STATE"
require_scratch_socket
rm -f "$SOCKET"
"$APP/Contents/Resources/keepd" >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
waited=0
while [ ! -S "$SOCKET" ] && [ "$waited" -lt 40 ]; do sleep 0.25; waited=$((waited + 1)); done

# Started, waited for, and put behind whatever was in front before it.
launch() {
    local front
    front=$(osascript -e 'tell application "System Events" to get bundle identifier of first application process whose frontmost is true' 2>/dev/null)
    : >"$WORK/app.log"
    KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
    APP_PID=$!
    local waited=0
    while ! grep -aqE "sidebar +order" "$WORK/app.log" && [ "$waited" -lt 120 ]; do
        sleep 0.25; waited=$((waited + 1))
    done
    sleep 1.5
    if [ -n "$front" ] && [ "$front" != "missing value" ]; then
        osascript -e "tell application id \"$front\" to activate" >/dev/null 2>&1
    fi
}
quit_app() {
    kill "$APP_PID" 2>/dev/null
    local waited=0
    while kill -0 "$APP_PID" 2>/dev/null && [ "$waited" -lt 40 ]; do sleep 0.25; waited=$((waited + 1)); done
    APP_PID=
}

stop_app
launch

level() { "$AXPRESS" "$APP_NAME" title keep.zoom.reset 2>/dev/null || echo "(none)"; }
enabled() { "$AXPRESS" "$APP_NAME" enabled "$1" 2>/dev/null || echo "(none)"; }
present() { "$AXPRESS" "$APP_NAME" frame "$1" >/dev/null 2>&1 && echo yes || echo no; }
written() {  # the size the app wrote down, or "none"
    python3 -c 'import json, sys
try: print(json.load(open(sys.argv[1])).get("fontSize", "none"))
except FileNotFoundError: print("none")' "$STATE/terminal.json"
}
grid() {  # grid <tab>: "colsxrows" the daemon was given for it
    "$APP/Contents/Resources/keep" ls 2>/dev/null \
        | sed -nE "s/^ +tab $1 .* ([0-9]+x[0-9]+) .*/\\1/p" | head -1
}
cols() { grid "$1" | cut -dx -f1; }
# A change reaches the daemon a moment after the app makes it: wait for the
# tab's grid to stop being $2, a few seconds at most.
grid_leaves() {  # grid_leaves <tab> <grid>
    local waited=0
    while [ "$(grid "$1")" = "$2" ] && [ "$waited" -lt 20 ]; do sleep 0.25; waited=$((waited + 1)); done
    sleep 0.3
}
grid_is() {  # grid_is <tab> <grid>
    local waited=0
    while [ "$(grid "$1")" != "$2" ] && [ "$waited" -lt 20 ]; do sleep 0.25; waited=$((waited + 1)); done
}
press() { "$AXPRESS" "$APP_NAME" press "$1"; sleep 0.4; }
chord() { "$SENDKEY" "$APP_PID" key "$@"; sleep 0.6; }
palette() {  # palette <filter>: run the palette's first command for it
    "$SENDKEY" "$APP_PID" key 35 shift cmd; sleep 0.8   # ⌘⇧P
    "$SENDKEY" "$APP_PID" text "$1"; sleep 0.6
    "$SENDKEY" "$APP_PID" key 36; sleep 0.8              # return
}

say ""
say "the zoom, above the sidebar"
check "it says 100%" "100%" "$(level)"
check "larger can be pressed" 1 "$(enabled keep.zoom.in)"
check "smaller can be pressed" 1 "$(enabled keep.zoom.out)"
read -r ix _ iw _ <<<"$("$AXPRESS" "$APP_NAME" frame keep.zoom.in)"
read -r tx _ _ _ <<<"$("$AXPRESS" "$APP_NAME" frame "Toggle Sidebar")"
check "it ends before the toggle" yes "$([ $((ix + iw)) -le "${tx:-0}" ] && echo yes || echo no)"
BASE=$(grid 1)
say "        (tab 1 at 100%: $BASE)"

say ""
say "the buttons"
press keep.zoom.in; grid_leaves 1 "$BASE"
check "+ says 110%" "110%" "$(level)"
check "+ writes 110% of 13 points" 14.3 "$(written)"
AT110=$(grid 1)
check "+ gives the tab fewer columns" yes "$([ "$(cols 1)" -lt "${BASE%x*}" ] && echo yes || echo no)"
press keep.zoom.out; grid_is 1 "$BASE"; press keep.zoom.out; grid_leaves 1 "$BASE"
check "- twice says 90%" "90%" "$(level)"
check "- twice writes 11.7" 11.7 "$(written)"
check "- gives it more columns than at 100%" yes "$([ "$(cols 1)" -gt "${BASE%x*}" ] && echo yes || echo no)"
AT90=$(grid 1)
press keep.zoom.reset; grid_leaves 1 "$AT90"
check "the percentage goes back to 100%" "100%" "$(level)"
check "and writes nothing down" none "$(written)"
check "and the tab is as it was" "$BASE" "$(grid 1)"

say ""
say "the chords, posted to the app alone"
chord 24 cmd; grid_leaves 1 "$BASE"
check "⌘= zooms in" "110% $AT110" "$(level) $(grid 1)"
chord 27 cmd; grid_leaves 1 "$AT110"
check "⌘- zooms out" "100% $BASE" "$(level) $(grid 1)"
chord 24 shift cmd; grid_leaves 1 "$BASE"
check "⌘+ zooms in" "110% $AT110" "$(level) $(grid 1)"
chord 29 cmd; grid_leaves 1 "$AT110"
check "⌘0 goes back to 100%" "100% $BASE" "$(level) $(grid 1)"

say ""
say "the View menu"
"$AXPRESS" "$APP_NAME" menu View "Zoom In"; sleep 0.4; grid_leaves 1 "$BASE"
check "Zoom In" "110% $AT110" "$(level) $(grid 1)"
"$AXPRESS" "$APP_NAME" menu View "Actual Size"; sleep 0.4; grid_leaves 1 "$AT110"
check "Actual Size" "100% $BASE" "$(level) $(grid 1)"

say ""
say "every tab, not the one on screen"
"$AXPRESS" "$APP_NAME" menu File "New Tab"; sleep 2
check "a second tab opens at 100%" "$BASE" "$(grid 2)"
press keep.zoom.in; grid_leaves 2 "$BASE"
check "+ on the second tab" "$AT110" "$(grid 2)"
"$AXPRESS" "$APP_NAME" menu Window "Show Tab 1"; sleep 0.4; grid_leaves 1 "$BASE"
check "the first tab is at 110% when it is shown" "$AT110" "$(grid 1)"
press keep.zoom.reset; grid_leaves 1 "$AT110"

say ""
say "the ends: 300% and 50%"
for _ in 1 2 3 4 5 6 7; do press keep.zoom.in; done
grid_leaves 1 "$BASE"; sleep 1
check "seven steps up is 300%" "300%" "$(level)"
check "39 points written" 39 "$(written)"
check "larger cannot be pressed" 0 "$(enabled keep.zoom.in)"
AT300=$(grid 1)
# Nothing in the menu to do, the chord goes on to the pane, which must not
# take it as its own font size: the pane would sit at a size of its own
# through every zoom after.
chord 24 cmd; sleep 1
check "⌘= at 300% leaves the tab alone" "300% $AT300" "$(level) $(grid 1)"
chord 24 shift cmd; sleep 1
check "⌘+ at 300% leaves the tab alone" "300% $AT300" "$(level) $(grid 1)"
for _ in $(seq 12); do press keep.zoom.out; done
grid_leaves 1 "$AT300"; sleep 1
check "twelve steps down is 50%" "50%" "$(level)"
check "smaller cannot be pressed" 0 "$(enabled keep.zoom.out)"
AT50=$(grid 1)
chord 27 cmd; sleep 1
check "⌘- at 50% leaves the tab alone" "50% $AT50" "$(level) $(grid 1)"
chord 29 cmd; grid_leaves 1 "$AT50"
check "⌘0 from 50%" "100% $BASE" "$(level) $(grid 1)"
chord 29 cmd; sleep 1
check "⌘0 at 100% leaves the tab alone" "100% $BASE" "$(level) $(grid 1)"

say ""
say "the palette's Bigger and Smaller text take the same steps"
palette "bigger text"; grid_leaves 1 "$BASE"
check "Bigger text" "110% 14.3" "$(level) $(written)"
palette "smaller text"; grid_leaves 1 "$AT110"
check "Smaller text" "100% none" "$(level) $(written)"

say ""
say "a shut sidebar takes the zoom with it"
press "Toggle Sidebar"; sleep 1
check "shut: no zoom" "no no no" "$(present keep.zoom.out) $(present keep.zoom.reset) $(present keep.zoom.in)"
# Where the toggle sits with nothing to ride: hard against the traffic
# lights. The zoom may never come nearer them than this.
read -r home _ _ _ <<<"$("$AXPRESS" "$APP_NAME" frame "Toggle Sidebar")"
press "Toggle Sidebar"; sleep 1
check "open again: the zoom is back" "yes yes yes" "$(present keep.zoom.out) $(present keep.zoom.reset) $(present keep.zoom.in)"

say ""
say "the next launch opens at the size it was left at"
press keep.zoom.in; grid_leaves 1 "$BASE"
quit_app
launch
grid_is 1 "$AT110"
check "it says 110%" "110%" "$(level)"
check "and the tab has 110%'s grid" "$AT110" "$(grid 1)"

say ""
say "a narrow sidebar keeps the buttons and drops the percentage"
quit_app
python3 - "$STATE/sidebar-state.json" <<'PY'
import json, sys
state = json.load(open(sys.argv[1]))
for window in state.values():
    window["width"] = 200
    window["isCollapsed"] = False
json.dump(state, open(sys.argv[1], "w"))
PY
launch
check "200 points wide: smaller, larger, no percentage" "yes yes no" \
    "$(present keep.zoom.out) $(present keep.zoom.in) $(present keep.zoom.reset)"
read -r nx _ _ _ <<<"$("$AXPRESS" "$APP_NAME" frame keep.zoom.out)"
read -r ix _ iw _ <<<"$("$AXPRESS" "$APP_NAME" frame keep.zoom.in)"
read -r tx _ _ _ <<<"$("$AXPRESS" "$APP_NAME" frame "Toggle Sidebar")"
check "no nearer the traffic lights than the toggle's home" yes \
    "$([ "${nx:-0}" -ge "${home:-9999}" ] && echo yes || echo no)"
check "and still clear of the toggle" yes "$([ $((ix + iw)) -le "${tx:-0}" ] && echo yes || echo no)"
press keep.zoom.out; grid_leaves 1 "$AT110"
check "and they still zoom: smaller from 110% is 100%" none "$(written)"

say ""
say "$PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
