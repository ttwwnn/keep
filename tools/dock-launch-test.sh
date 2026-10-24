#!/usr/bin/env bash
#
# The app opened the way the Dock opens it: launchd's dozen variables and no
# LANG, which a terminal passes along and the Dock does not. Up to 24/10/2026
# the app closed itself a second after opening like that, every time: it wrote
# a variable into its environment after `ghostty_init`, which can move the
# environment and free the one libghostty holds (tools/env-test.sh). Whether it
# moves depends on how many variables there are, so the app is opened with each
# count from 11 to 26 and must still be running 6 s later every time. The build
# before the fix closed with 12 and with 16.
#
# KeepDev on a daemon of its own (scratch socket and state folders): your Keep
# and your daemon are left alone. Each launch takes the front for a moment; it
# is handed back at the end. About two minutes.
#
#   tools/dock-launch-test.sh   (SKIP_BUILD=1 to use the KeepDev built last,
#                                KEEP_TEST_APP=<.app> to open another build)
set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=${KEEP_TEST_APP:-$(dev_app)}
APP_NAME=$(app_name "$APP")
BIN=$APP/Contents/MacOS/$APP_NAME
WORK=$(mktemp -d /tmp/keep-dock-XXXXXX)
SOCKET=/tmp/kdl-$$.sock
export KEEP_SOCKET=$SOCKET
require_scratch_socket
PASSED=0
FAILED=0
FRONT=$(osascript -e 'tell application "System Events" to get bundle identifier of first process whose frontmost is true' 2>/dev/null)

say() { printf '%s\n' "$*"; }
stop() {  # stop <pid>: the app and what it started, nothing else
    local pid
    for pid in $(descendants "$1"); do kill "$pid" 2>/dev/null; done
    kill -9 "$1" 2>/dev/null
    wait "$1" 2>/dev/null
}
cleanup() {
    [ -n "${APP_PID:-}" ] && stop "$APP_PID"
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
    rm -f "$SOCKET"
    [ -n "$FRONT" ] && osascript -e "tell application id \"$FRONT\" to activate" >/dev/null 2>&1
    [ -n "${KEEP_WORK:-}" ] && say "kept: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT INT TERM

if [ -z "${KEEP_TEST_APP:-}" ] && [ -z "${SKIP_BUILD:-}" ]; then
    say "building $APP_NAME, the client and the daemon (yours is left alone)"
    ./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }
fi
[ -x "$BIN" ] || { say "no app at $APP"; exit 1; }

SHELL=/bin/sh "$APP/Contents/Resources/keepd" >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
waited=0
while [ ! -S "$SOCKET" ] && [ "$waited" -lt 40 ]; do sleep 0.25; waited=$((waited + 1)); done
[ -S "$SOCKET" ] || { say "the daemon did not come up"; exit 1; }

say "opening $APP_NAME as the Dock does, with 11 to 26 variables"
for count in $(seq 11 26); do
    state=$WORK/state-$count
    mkdir -p "$state"
    # What the Dock gives an app, less what the test has no use for, and
    # what keeps it off your daemon, your state and your logins.
    vars=(HOME="$HOME" USER="$USER" LOGNAME="$USER" SHELL=/bin/zsh
        TMPDIR="$(getconf DARWIN_USER_TEMP_DIR)" PATH=/usr/bin:/bin:/usr/sbin:/sbin
        KEEP_SOCKET="$SOCKET" KEEP_STATE_DIR="$state" KEEP_AI_USAGE_HOME=/var/empty
        KEEP_IA_BIN=/var/empty KEEP_WORKTREES_BIN=/var/empty)
    pad=0
    while [ "${#vars[@]}" -lt "$count" ]; do pad=$((pad + 1)); vars+=("KEEP_TEST_PAD$pad=x"); done
    env -i "${vars[@]}" "$BIN" >"$WORK/app-$count.log" 2>&1 &
    APP_PID=$!
    running=yes
    for _ in $(seq 1 24); do
        sleep 0.25
        kill -0 "$APP_PID" 2>/dev/null || { running=no; break; }
    done
    if [ "$running" = yes ]; then
        say "  ok    $count variables: still running after 6 s"
        PASSED=$((PASSED + 1))
        stop "$APP_PID"
        # And the core was still told where this build keeps its pictures of
        # the tabs (`KeepCLI.environment`): without it, it looks next to the
        # made-up home's state, where nothing can be written.
        if [ -f "$state/restauro/restaurado.json" ]; then
            PASSED=$((PASSED + 1))
        else
            say "  FAIL  $count variables: no pictures of the tabs in its own state folder"
            FAILED=$((FAILED + 1))
        fi
    else
        wait "$APP_PID"
        say "  FAIL  $count variables: closed by itself, status $?"
        FAILED=$((FAILED + 1))
    fi
    APP_PID=
    sleep 1
done

say ""
say "$PASSED ok, $FAILED failed"
[ "$FAILED" -eq 0 ]
