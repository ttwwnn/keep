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
# What the app remembers between launches goes in here too, so this test
# neither reads the arrangement of whoever is running it nor leaves its own
# behind. It used to scrub its leftovers out of their real file afterwards,
# which is a repair, not a boundary.
export KEEP_STATE_DIR=$WORK/state
mkdir -p "$KEEP_STATE_DIR"
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
shape() { trace | grep "row " | tail -1; }

# Brought to the front again before anything is clicked.
#
# A click on a window that is not key spends itself activating that window, so
# a probe that assumed focus from a `frontmost` issued half a minute and a
# keystroke ago reports "nothing reached a tab" about an aim that was exact.
focus_app() {
    osascript -e 'tell application "System Events" to set frontmost of process "Keep" to true' \
        >/dev/null 2>&1
    sleep 1
}
last_order() { trace | grep "dropped, order now" | tail -1 | sed -E 's/.*now \[(.*)\]/\1/'; }

# ------------------------------------------------------- where the tabs are
#
# Put the row somewhere known instead of working out where it is.
#
# The row begins where the content does — past the sidebar, when there is one —
# so where it starts depends on furniture this test does not own. Deducing it
# put the aim one tab off; hunting for it by clicking walked the pointer along
# the row and eventually pressed a close button, and a test that quietly closes
# a tab it is about to count is worse than one that cannot find the row.
#
# So: collapse the sidebar. The row then spans the whole window, its left edge
# is the window's, and `clear` is the fixed gap the chrome keeps. Nothing is
# inferred, and the state is the test's own rather than whatever the person
# using the app happened to leave behind.
collapse_sidebar() {
    local attempt
    for attempt in 1 2; do
        [ "$(sed -E 's/.*clear=([0-9]+).*/\1/' <<<"$(shape)")" != 0 ] && return 0
        "$SENDKEY" "$APP_PID" key 11 cmd    # cmd-b
        sleep 2
    done
    [ "$(sed -E 's/.*clear=([0-9]+).*/\1/' <<<"$(shape)")" != 0 ]
}
collapse_sidebar || { say "could not collapse the sidebar; the row is still behind it"; exit 1; }

read -r WX WY WW WH < <("$MOUSE" frame) || { say "no Keep window on screen"; exit 1; }
SHAPE=$(shape)
CLEAR=$(sed -E 's/.*clear=([0-9]+).*/\1/' <<<"$SHAPE")
SLOT=$(sed -E 's/.*slot=([0-9]+).*/\1/' <<<"$SHAPE")
COUNT=$(sed -E 's/.*tabs=([0-9]+).*/\1/' <<<"$SHAPE")
[ "${COUNT:-0}" = 3 ] || { say "wanted three tabs, the row has '${COUNT:-none}'"; exit 1; }
LEFT=$WX

# The middle of the first tab: far from the close button, which sits within
# fourteen points of a cell's leading edge.
focus_app
for DY in 22 30 16; do
    PROBE_Y=$((WY + DY))
    BEFORE=$(trace | grep -c "press on")
    "$MOUSE" drag $((LEFT + CLEAR + SLOT / 2)) "$PROBE_Y" $((LEFT + CLEAR + SLOT / 2)) "$PROBE_Y" 2 20
    sleep 0.8
    [ "$(trace | grep -c "press on")" -gt "$BEFORE" ] && { ROW_Y=$PROBE_Y; break; }
done
if [ -z "${ROW_Y:-}" ]; then
    say "no press reached a tab; the row is not where the arithmetic says"
    say "  window     ${WX},${WY} ${WW}x${WH}"
    say "  row traced $SHAPE"
    say "  aimed at   $((LEFT + CLEAR + SLOT / 2)) across, ${PROBE_Y:-?} down"
    say "  last row line now: $(shape)"
    exit 1
fi
say ""
say "the row: ${SLOT}pt a tab, ${CLEAR}pt of chrome before the first, at y=$ROW_Y"

slot() { echo $((LEFT + CLEAR + $1 * SLOT + SLOT / 2)); }

# Two positions in the log is the drag's own start and finish being sampled at
# slightly different moments; a window being carried takes dozens. Asked while
# the drag is happening, because a tiling window manager puts a window that
# moved straight back and the evidence is gone a moment later.
drag_slots() {
    focus_app
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
# Same reason as `reorder`: a window that a tiling manager puts back before
# the sampler notices has not disproved anything.
moved_window() {   # moved_window <what> <x> <y> <dy>
    local step
    for step in $STEPS $STEPS; do
        window_travelled && { check "$1" "it moved" "it moved"; return; }
        say "        (still there; pushing $step across)"
        "$MOUSE" watch 4 >"$WORK/window.log" 2>&1 &
        local watcher=$!
        "$MOUSE" drag "$2" "$3" $(( $2 + step )) $(( $3 + $4 )) 25 18
        wait "$watcher" 2>/dev/null
        sleep 1
    done
    window_travelled && check "$1" "it moved" "it moved" \
        || check "$1" "it moved" "it stayed put"
}

check_still() {
    if window_travelled; then
        check "$1" "the window held still" \
            "it was dragged across $(awk '{print $3}' "$WORK/window.log" | sort -u | wc -l | tr -d ' ') positions"
    else
        check "$1" "the window held still" "the window held still"
    fi
}

# Which way a window can actually be pushed is not knowable in advance.
#
# A window against the right edge of the screen will not go further right, and
# on a second display — where the coordinates are negative and the desktop
# bounds do not describe the screen the window is on — working out which edge
# it is against gets it wrong. So the direction is not computed: the first
# attempt goes one way and the retry goes the other, and a window that moves
# either way has proved the point.
STEPS="160 -160"

# ------------------------------------------------------------ moving a tab

say ""
say "moving a tab"
# Tried more than once before being believed.
#
# Driving a real window manager with synthesised events is not repeatable:
# the same drag, run twice against the same build, has come back right and
# wrong. AeroSpace re-tiles mid-gesture, the window crosses to another
# display between one command and the next, and a pointer that has to be
# posted rather than moved by a hand arrives at its own pace. A drag that
# succeeds on any attempt is a drag that works; one that fails three times
# running is a defect. Anything in between was never evidence.
reorder() {   # reorder <from-slot> <to-slot> <what> <expected>
    local attempt
    for attempt in 1 2 3; do
        drag_slots "$1" "$2"
        [ "$(last_order)" = "$4" ] && { check "$3" "$4" "$4"; return; }
        say "        (attempt $attempt gave [$(last_order)], trying again)"
    done
    check "$3" "$4" "$(last_order)"
}

reorder 0 1 "one place right" "2, 1, 3"
check_still "the window stayed where it was while the tab moved"

reorder 1 0 "and back again" "1, 2, 3"

reorder 0 2 "two places at once" "2, 3, 1"

# ------------------------------------------------- what a titlebar is still for

say ""
say "what the rest of the row is still for"
read -r WX WY WW WH < <("$MOUSE" frame); ROW_Y=$((WY + (ROW_Y - WY)))
"$MOUSE" watch 4 >"$WORK/window.log" 2>&1 &
WATCHER=$!
"$MOUSE" drag $((WX + WW - 4)) "$ROW_Y" $((WX + WW - 164)) $((ROW_Y + 40)) 25 18
wait "$WATCHER" 2>/dev/null
sleep 1
moved_window "the bare end of the row drags the window" \
    $((WX + WW - 4)) "$ROW_Y" 40

say ""
say "a window with one tab"
"$SENDKEY" "$APP_PID" key 13 cmd; sleep 2     # cmd-w
"$SENDKEY" "$APP_PID" key 13 cmd; sleep 2     # cmd-w
read -r WX WY WW WH < <("$MOUSE" frame)
"$MOUSE" watch 4 >"$WORK/window.log" 2>&1 &
WATCHER=$!
# Squarely on the title, which with one tab is the whole row.
"$MOUSE" drag $((WX + WW / 2)) $((WY + 22)) $((WX + WW / 2 - 160)) $((WY + 62)) 25 18
wait "$WATCHER" 2>/dev/null
sleep 1
moved_window "a lone title is still a title bar" \
    $((WX + WW / 2)) $((WY + 22)) 40

# ---------------------------------------------------------------------- done

say ""
if [ "$FAILED" -eq 0 ]; then
    say "$PASSED checks passed"
    exit 0
fi
say "$FAILED of $((PASSED + FAILED)) checks failed"
exit 1
