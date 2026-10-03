#!/usr/bin/env bash
#
# The zoom above the sidebar (UI/ZoomControl.swift), in the app: its buttons,
# its chords and its menu items change the size of every tab's text, step by
# Chrome's steps; it stops at 50% and at 300%, the View menu greyed there and
# the chord going no further than the terminal; it tells no program the
# colours changed; it goes with a shut sidebar, loses its percentage in a
# narrow one, opens next time at the size it was left at, and takes for 100%
# the size the person's own config gives, from whichever file.
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
# (tools/sendkey.swift) — which reach its terminal as well as its menu, in
# front or not — and the front is handed back to whatever had it at each
# launch, so the test runs behind the app you are working in. Your app, its
# daemon and its settings are never touched.

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
FRONT=
launch() {
    FRONT=$(osascript -e 'tell application "System Events" to get bundle identifier of first application process whose frontmost is true' 2>/dev/null)
    : >"$WORK/app.log"
    KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
    APP_PID=$!
    local waited=0
    while ! grep -aqE "sidebar +order" "$WORK/app.log" && [ "$waited" -lt 120 ]; do
        sleep 0.25; waited=$((waited + 1))
    done
    sleep 1.5
    give_back
}
give_back() {
    if [ -n "$FRONT" ] && [ "$FRONT" != "missing value" ]; then
        osascript -e "tell application id \"$FRONT\" to activate" >/dev/null 2>&1
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
item() { "$AXPRESS" "$APP_NAME" menuenabled View "$1" 2>/dev/null || echo "(none)"; }
# Whether an item of the View menu can be chosen, once AppKit has caught up:
# it works the menu out again a moment after a change (within a second,
# measured), not at the change. Opened by hand, a menu is worked out as it
# opens; read or pressed through the accessibility tree, it is whatever it
# was last — Actual Size, greyed at 100%, was still greyed just after 110%.
item_is() {  # item_is <item> <0|1>: what it says, waiting up to 3 s for <0|1>
    local waited=0 got
    while got=$(item "$1"); [ "$got" != "$2" ] && [ "$waited" -lt 12 ]; do
        sleep 0.25; waited=$((waited + 1))
    done
    printf '%s\n' "$got"
}
choose() { item_is "$1" 1 >/dev/null; "$AXPRESS" "$APP_NAME" menu View "$1"; sleep 0.4; }
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
screen() {  # screen <tab>: what the daemon holds on that tab's screen
    python3 tools/screen.py "$("$APP/Contents/Resources/keep" ls 2>/dev/null | awk 'NR == 1 { print $1 }')" "$1" 2>/dev/null
}
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
check "View: Zoom In and Zoom Out can be chosen, Actual Size is greyed" "1 1 0" \
    "$(item_is "Zoom In" 1) $(item_is "Zoom Out" 1) $(item_is "Actual Size" 0)"
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
choose "Zoom In"; grid_leaves 1 "$BASE"
check "Zoom In" "110% $AT110" "$(level) $(grid 1)"
choose "Actual Size"; grid_leaves 1 "$AT110"
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
check "View: Zoom In is greyed, Actual Size can be chosen" "0 1" \
    "$(item_is "Zoom In" 0) $(item_is "Actual Size" 1)"
AT300=$(grid 1)
# A greyed item hands its chord on to the terminal, where libghostty binds it
# to the size of the pane alone — and a pane sized so stops following the
# zoom — unless Keep's config tells it to ignore the chord. First that what
# is posted here reaches the terminal at all, or the checks after it test
# nothing: typed text does, to the shell's command line.
"$SENDKEY" "$APP_PID" text "zoomprobe"; sleep 1
check "what is typed reaches the terminal" yes "$(screen 1 | grep -q zoomprobe && echo yes || echo no)"
chord 32 ctrl   # ⌃U: the probe off the command line
# And the shell asks to be told when the ground goes light or dark, as
# Claude Code does (mode 2031): see "no step of the zoom" below.
"$SENDKEY" "$APP_PID" text "printf '\\033[?2031h'"; "$SENDKEY" "$APP_PID" key 36; sleep 1.5
TOLD_FROM=$(wc -l <"$WORK/app.log")
chord 24 cmd; sleep 1
check "⌘= at 300% does not reach the pane" "300% $AT300" "$(level) $(grid 1)"
chord 24 shift cmd; sleep 1
check "⌘+ at 300% does not reach the pane" "300% $AT300" "$(level) $(grid 1)"
for _ in $(seq 12); do press keep.zoom.out; done
grid_leaves 1 "$AT300"; sleep 1
check "twelve steps down is 50%" "50%" "$(level)"
check "smaller cannot be pressed" 0 "$(enabled keep.zoom.out)"
check "View: Zoom Out is greyed" 0 "$(item_is "Zoom Out" 0)"
AT50=$(grid 1)
chord 27 cmd; sleep 1
check "⌘- at 50% does not reach the pane" "50% $AT50" "$(level) $(grid 1)"
chord 29 cmd; grid_leaves 1 "$AT50"
check "⌘0 from 50%" "100% $BASE" "$(level) $(grid 1)"
check "View: Actual Size is greyed again" 0 "$(item_is "Actual Size" 0)"
chord 29 cmd; sleep 1
check "⌘0 at 100% does not reach the pane" "100% $BASE" "$(level) $(grid 1)"

say ""
say "the palette's Bigger and Smaller text take the same steps"
palette "bigger text"; grid_leaves 1 "$BASE"
check "Bigger text" "110% 14.3" "$(level) $(written)"
palette "smaller text"; grid_leaves 1 "$AT110"
check "Smaller text" "100% none" "$(level) $(written)"

say ""
say "no step of the zoom tells a program the ground changed"
# A ground said to have changed is told to every program that asked
# (`tellScheme`, a second after). Fifteen steps since the shell asked — the
# buttons, ⌘0, the palette — and not one of them a change of colour.
sleep 2
check "the shell that asked was told nothing" 0 \
    "$(tail -n +"$((TOLD_FROM + 1))" "$WORK/app.log" | grep -ac 'scheme .* told')"

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
say "100% is the size the person's config gives, in whichever file libghostty finds it"
quit_app
# A font-size in config.ghostty under XDG_CONFIG_HOME: one of the four files
# libghostty reads, and one a reading of ~/.config/ghostty/config alone never
# saw. Application Support is read after it and would win, so the section
# stands aside when that sets a size of its own.
SUPPORT="$HOME/Library/Application Support/com.mitchellh.ghostty"
if grep -qsE '^[[:space:]]*font-size[[:space:]]*=' "$SUPPORT/config" "$SUPPORT/config.ghostty"; then
    say "  skip  your Application Support config sets a font-size, which would win"
else
    mkdir -p "$WORK/xdg/ghostty"
    echo "font-size = 16" >"$WORK/xdg/ghostty/config.ghostty"
    rm -f "$STATE/terminal.json"
    python3 - "$STATE/sidebar-state.json" <<'INNER'
import json, sys
state = json.load(open(sys.argv[1]))
for window in state.values():
    window["width"] = 250
json.dump(state, open(sys.argv[1], "w"))
INNER
    export XDG_CONFIG_HOME=$WORK/xdg
    launch
    unset XDG_CONFIG_HOME
    check "16 points says 100%" "100%" "$(level)"
    check "and the app took 16 points for its own" yes \
        "$(grep -aqE 'terminal font: .*@16\.0pt \(own 16\.0pt\)' "$WORK/app.log" && echo yes || echo no)"
    AT16=$(grid 1)
    press keep.zoom.in; grid_leaves 1 "$AT16"
    check "+ is 110% of 16" "110% 17.6" "$(level) $(written)"
    check "and the text grew: fewer columns" yes "$([ "$(cols 1)" -lt "${AT16%x*}" ] && echo yes || echo no)"
fi

say ""
say "$PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
