#!/usr/bin/env bash
#
# What a drag on the tab strip does.
#
# Reordering tabs was written, shipped, and reported broken twice, and both
# repairs aimed at the wrong thing — the arithmetic that decides which place a
# tab lands in — because from inside the app the drag looked fine: the press
# arrived, the tab was picked up, the tab was put down. What was actually
# happening is invisible from there. The row lives in the titlebar, a press in
# a titlebar hands the window server a drag of its own, and the window then
# travels with the pointer. The pointer therefore never moves *relative to the
# window*, `locationInWindow` reads the same value from the first event to the
# last, and a row that measures travel in window coordinates is told, quite
# correctly, that the finger never moved.
#
# So this test asks the question from outside: it drags with events the window
# server routes, the way a hand does, and it watches the window's position
# while the drag is happening. A test that only asserted about the resulting
# order would have passed against a build where the window ran away, because
# the order does change if you drag far enough — it just never changes for the
# person doing it.
#
#   tools/tabs-test.sh
#
# It takes the mouse over for about a minute. Nothing is mocked: a daemon of
# its own on a scratch socket, a window, three shells, and real drags.
#
# It stops the running Keep and does not start it again: this is a test, and
# the app it leaves behind would be pointed at a socket that no longer exists.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=apps/macos/build/Build/Products/Debug/Keep.app
BIN=$APP/Contents/MacOS/Keep
SOCKET=/tmp/keep-tabs-$$.sock
WORK=$(mktemp -d /tmp/keep-tabs-XXXXXX)
MOUSE=$WORK/mousedrag
SENDKEY=$WORK/sendkey
WORKSPACE=tabs
PASSED=0
FAILED=0

cleanup() {
    [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
    restore_app "$APP"
    # The row's order is remembered on disk, beside the order of the windows
    # the person actually uses. A test that left its own behind would be
    # editing their session.
    python3 - "$WORKSPACE" <<'PY' 2>/dev/null
import json, os, sys
path = os.path.expanduser("~/Library/Application Support/Keep/tab-order.json")
try:
    with open(path) as f:
        saved = json.load(f)
except (OSError, ValueError):
    sys.exit(0)
if saved.pop(sys.argv[1], None) is not None:
    with open(path, "w") as f:
        json.dump(saved, f)
PY
    rm -rf "$WORK" "$SOCKET"
}
# INT and TERM as well as EXIT: a test that is interrupted has still
# taken the person's app away, and leaving it taken is how a stopped
# test looks exactly like a broken app.
trap cleanup EXIT INT TERM

say() { printf '%s\n' "$*"; }

[ -x "$BIN" ] || { say "no app at $BIN — build it first"; exit 1; }

say "building the client, the daemon and the mouse"
cargo build --release -p keep -p keepd >/dev/null 2>&1 || { say "cargo build failed"; exit 1; }
swiftc -O tools/mousedrag.swift -o "$MOUSE" 2>/dev/null || { say "could not build mousedrag"; exit 1; }
swiftc -O tools/sendkey.swift -o "$SENDKEY" 2>/dev/null || { say "could not build sendkey"; exit 1; }

export KEEP_SOCKET=$SOCKET
require_scratch_socket
rm -f "$SOCKET"
./target/release/keepd >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
sleep 2
./target/release/keep new "$WORKSPACE" >/dev/null 2>&1 || { say "could not make a workspace"; exit 1; }

# Yours steps aside for the one under test and is started again at the end.
# Your daemon is never touched: it holds your sessions throughout.
say "starting the app on its own daemon (yours comes back at the end)"
pkill -x Keep 2>/dev/null
sleep 2
KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
APP_PID=$!
sleep 11
osascript -e 'tell application "System Events" to set frontmost of process "Keep" to true' >/dev/null 2>&1
sleep 1

# Three tabs, so that a tab has somewhere to go in both directions.
"$SENDKEY" "$APP_PID" key 17 cmd; sleep 2     # cmd-t
"$SENDKEY" "$APP_PID" key 17 cmd; sleep 2     # cmd-t

trace() { grep -a "  strip " "$WORK/app.log"; }
last_order() { trace | grep "dropped, order now" | tail -1 | sed -E 's/.*now \[(.*)\]/\1/'; }

# ------------------------------------------------------- where the tabs are
#
# Asked of the app rather than assumed: the row's arithmetic depends on
# whether the sidebar is showing, and a test that guessed would click between
# two tabs and report on nothing. The app traces the row's shape; one press
# supplies the rest, since the difference between where a press was posted and
# where the row says it landed is the window's own left edge.

read -r WX WY WW WH < <("$MOUSE" frame) || { say "no Keep window on screen"; exit 1; }
SHAPE=$(trace | grep "row " | tail -1)
CLEAR=$(sed -E 's/.*clear=([0-9]+).*/\1/' <<<"$SHAPE")
SLOT=$(sed -E 's/.*slot=([0-9]+).*/\1/' <<<"$SHAPE")
COUNT=$(sed -E 's/.*tabs=([0-9]+).*/\1/' <<<"$SHAPE")
[ "${COUNT:-0}" = 3 ] || { say "wanted three tabs, the row has '${COUNT:-none}'"; exit 1; }

# Worked out rather than hunted for. `clear` is measured in the row's own
# coordinates and the row begins where the content does — past the sidebar,
# when there is one — so a press computed from the window's left edge lands
# on the sidebar instead. But the row ends at the window's right edge, and
# its shape is traced, so where it starts is arithmetic: the window's right
# edge less the row's whole width.
#
# Clicking about to find it, which is what this did before, walks the
# pointer along a row of tabs and eventually presses one of their close
# buttons — and a test that quietly closes a tab it is about to count is
# worse than one that cannot find the row at all.
STRIP_WIDTH=$((CLEAR + SLOT * COUNT + 46))
LEFT=$((WX + WW - STRIP_WIDTH))

# The middle of the first tab: far from the close button, which sits within
# fourteen points of a cell's leading edge.
for DY in 22 30 16; do
    PROBE_Y=$((WY + DY))
    BEFORE=$(trace | grep -c "press on")
    "$MOUSE" drag $((LEFT + CLEAR + SLOT / 2)) "$PROBE_Y" $((LEFT + CLEAR + SLOT / 2)) "$PROBE_Y" 2 20
    sleep 0.8
    [ "$(trace | grep -c "press on")" -gt "$BEFORE" ] && { ROW_Y=$PROBE_Y; break; }
done
[ -n "${ROW_Y:-}" ] || { say "no press reached a tab; the row is not where the arithmetic says"; exit 1; }
say ""
say "the row: ${SLOT}pt a tab, ${CLEAR}pt of chrome before the first, at y=$ROW_Y"

slot() { echo $((LEFT + CLEAR + $1 * SLOT + SLOT / 2)); }

# Two positions in the log is the drag's own start and finish being sampled at
# slightly different moments; a window being carried takes dozens. Asked while
# the drag is happening, because a tiling window manager puts a window that
# moved straight back and the evidence is gone a moment later.
drag_slots() {
    "$MOUSE" watch 4 >"$WORK/window.log" 2>&1 &
    local watcher=$!
    "$MOUSE" drag "$(slot $1)" "$ROW_Y" "$(slot $2)" "$ROW_Y" 30 18
    wait "$watcher" 2>/dev/null
    sleep 1.2
}
window_travelled() {
    [ "$(awk '{print $3}' "$WORK/window.log" | sort -u | wc -l | tr -d ' ')" -gt 2 ]
}

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
check_still() {
    if window_travelled; then
        check "$1" "the window held still" \
            "it was dragged across $(awk '{print $3}' "$WORK/window.log" | sort -u | wc -l | tr -d ' ') positions"
    else
        check "$1" "the window held still" "the window held still"
    fi
}

# ------------------------------------------------------------ moving a tab

say ""
say "moving a tab"
drag_slots 0 1
check "one place right"      "2, 1, 3" "$(last_order)"
check_still "the window stayed where it was while the tab moved"

drag_slots 1 0
check "and back again"       "1, 2, 3" "$(last_order)"

drag_slots 0 2
check "two places at once"   "2, 3, 1" "$(last_order)"

# ------------------------------------------------- what a titlebar is still for

say ""
say "what the rest of the row is still for"
read -r WX WY WW WH < <("$MOUSE" frame); ROW_Y=$((WY + (ROW_Y - WY)))
"$MOUSE" watch 4 >"$WORK/window.log" 2>&1 &
WATCHER=$!
"$MOUSE" drag $((WX + WW - 4)) "$ROW_Y" $((WX + WW + 156)) $((ROW_Y + 40)) 25 18
wait "$WATCHER" 2>/dev/null
sleep 1
if window_travelled; then
    check "the bare end of the row drags the window" "it moved" "it moved"
else
    check "the bare end of the row drags the window" "it moved" "it stayed put"
fi

say ""
say "a window with one tab"
"$SENDKEY" "$APP_PID" key 13 cmd; sleep 2     # cmd-w
"$SENDKEY" "$APP_PID" key 13 cmd; sleep 2     # cmd-w
read -r WX WY WW WH < <("$MOUSE" frame)
"$MOUSE" watch 4 >"$WORK/window.log" 2>&1 &
WATCHER=$!
# Squarely on the title, which with one tab is the whole row.
"$MOUSE" drag $((WX + WW / 2)) $((WY + 22)) $((WX + WW / 2 + 160)) $((WY + 62)) 25 18
wait "$WATCHER" 2>/dev/null
sleep 1
if window_travelled; then
    check "a lone title is still a title bar" "it moved" "it moved"
else
    check "a lone title is still a title bar" "it moved" "it stayed put"
fi

# ---------------------------------------------------------------------- done

say ""
if [ "$FAILED" -eq 0 ]; then
    say "$PASSED checks passed"
    exit 0
fi
say "$FAILED of $((PASSED + FAILED)) checks failed"
exit 1
