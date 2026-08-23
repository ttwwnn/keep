#!/usr/bin/env bash
#
# What a second window is, from outside the app.
#
# The app spent its whole life with exactly one window, on purpose: every bug
# the old shell had — flicker on switch, focus falling to another app, fights
# with a tiling window manager — came from windows being made and unmade as
# the price of an ordinary switch. A second window on request is a different
# thing from that, but only if it stays a different thing, and the difference
# is not visible from inside: what matters is whether the *first* window moved
# when the second appeared, whether the shells outlive a window closing, and
# whether the count and the geometry after a relaunch are the ones the app
# asked for rather than whatever AppKit felt like reopening.
#
# So this asks the window server, which is the only account that does not
# depend on the app agreeing with itself.
#
#   tools/windows-test.sh
#
# Nothing is mocked: a daemon of its own on a scratch socket, two windows,
# real shells, real keystrokes. It takes about two minutes.
#
# It stops the running Keep and starts it again at the end, pointed back at
# the daemon it was using. Your daemon is never touched.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=apps/macos/build/Build/Products/Debug/Keep.app
BIN=$APP/Contents/MacOS/Keep
SOCKET=/tmp/keep-windows-$$.sock
WORK=$(mktemp -d /tmp/keep-windows-XXXXXX)
MOUSE=$WORK/mousedrag
SENDKEY=$WORK/sendkey
WORKSPACE=windows
PASSED=0
FAILED=0

cleanup() {
    [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
    restore_app "$APP"
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

[ -x "$BIN" ] || { say "no app at $BIN — build it first"; exit 1; }

say "building the client, the daemon and the mouse"
cargo build --release -p keep -p keepd >/dev/null 2>&1 || { say "cargo build failed"; exit 1; }
bundle_binaries "$APP" || { say "could not put the fresh binaries in the bundle"; exit 1; }
swiftc -O tools/mousedrag.swift -o "$MOUSE" 2>/dev/null || { say "could not build mousedrag"; exit 1; }
swiftc -O tools/sendkey.swift -o "$SENDKEY" 2>/dev/null || { say "could not build sendkey"; exit 1; }

export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$WORK/state
mkdir -p "$KEEP_STATE_DIR"
require_scratch_socket
rm -f "$SOCKET"
./target/release/keepd >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
sleep 2
./target/release/keep new "$WORKSPACE" >/dev/null 2>&1 || { say "could not make a workspace"; exit 1; }

launch_app() {
    KEEP_TRACE=1 "$BIN" >>"$WORK/app.log" 2>&1 &
    APP_PID=$!
    sleep 11
    focus_app
}
focus_app() {
    osascript -e 'tell application "System Events" to set frontmost of process "Keep" to true' \
        >/dev/null 2>&1
    sleep 1
}
windows() { "$MOUSE" windows | sort -n; }
# Whether somebody else is deciding where windows go. AeroSpace re-tiles the
# moment a second window appears, so on a machine running one the geometry is
# not the app's to be judged on — and a check that fails there is a check that
# gets ignored everywhere.
tiling_manager() { pgrep -x AeroSpace >/dev/null 2>&1 || pgrep -x yabai >/dev/null 2>&1; }
window_count() { windows | wc -l | tr -d ' '; }
# One window's geometry, by its place in the list rather than its id: a
# relaunch issues new ids for the same windows.
geometry() { windows | awk -v n="$1" 'NR==n {print $2, $3, $4, $5}'; }
# How many clients the daemon has on the first tab. Asked of the daemon
# rather than of the app, because "both windows are showing it" is a claim
# about the shell, and the app is the thing under test.
viewers() {
    local mark
    mark=$(./target/release/keep ls | awk '/^  tab 1 /{print $NF}')
    case "$mark" in
        attached) echo 1 ;;
        # "x2" and friends: everything up to the last non-digit, dropped.
        *[0-9]) printf '%s\n' "${mark##*[!0-9]}" ;;
        *) echo 0 ;;
    esac
}
# Wait for a count instead of racing the client that is still starting.
viewers_settle() {  # viewers_settle <wanted>
    local n
    for _ in 1 2 3 4 5 6 7 8; do
        n=$(viewers)
        [ "$n" = "$1" ] && break
        sleep 1
    done
    printf '%s\n' "$n"
}
tab_count() { ./target/release/keep ls | grep -c "^  tab "; }

say "starting the app on its own daemon (yours comes back at the end)"
stop_app
launch_app

# --------------------------------------------------------- a second window
say ""
say "asking for a second window"
FIRST_BEFORE=$(geometry 1)
"$SENDKEY" "$APP_PID" key 45 cmd shift; sleep 3      # cmd-shift-n
check "there are two windows" 2 "$(window_count)"
# The whole point of the rule this change amends: making a window must not
# disturb the window you were using.
if tiling_manager; then
    say "  skip  the first window stayed where it was — a tiling window"
    say "        manager is running, and the geometry is its call, not ours"
else
    check "the first window stayed where it was" "$FIRST_BEFORE" "$(geometry 1)"
fi

# ------------------------------------------------- the same tab, mirrored
#
# The new window carries no workspaces; ⌘P and return bring the first one it
# offers into it, which is the tab window one is already showing. Whether it
# really is the same tab, rather than a picture of one, is a question for the
# daemon: it counts the clients watching a shell, and a mirrored tab has two.
say ""
say "pointing it at the same tab"
"$SENDKEY" "$APP_PID" key 35 cmd; sleep 2            # cmd-p
"$SENDKEY" "$APP_PID" key 36; sleep 3                # return
check "two windows are watching one shell" 2 "$(viewers_settle 2)"

# ------------------------------------------------------ across a relaunch
say ""
say "quitting and starting again"
WANTED_COUNT=$(window_count)
TABS_BEFORE=$(tab_count)
"$SENDKEY" "$APP_PID" key 12 cmd; sleep 4            # cmd-q
[ "$(window_count)" = 0 ] || { say "the app did not quit"; exit 1; }
launch_app
check "both windows came back" "$WANTED_COUNT" "$(window_count)"
check "nothing was closed on the way" "$TABS_BEFORE" "$(tab_count)"
# Not just the count: the second window came back pointed at the tab it was
# left on, which is the part a restored window can get wrong while still
# looking right.
check "and both are watching that shell again" 2 "$(viewers_settle 2)"

# ---------------------------------------------- closing a window kills nothing
say ""
say "closing one of them"
"$SENDKEY" "$APP_PID" key 13 cmd alt; sleep 3     # alt-cmd-w
check "one window is left" 1 "$(window_count)"
check "every shell is still running" "$TABS_BEFORE" "$(tab_count)"
check "with one window watching, not two" 1 "$(viewers_settle 1)"

say ""
if [ "$FAILED" = 0 ]; then
    say "$PASSED checks passed"
else
    say "$PASSED passed, $FAILED failed"
fi
[ "$FAILED" = 0 ]
