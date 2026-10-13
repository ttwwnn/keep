#!/usr/bin/env bash
#
# Hibernation (Display/Hibernation.swift, Model/HibernationPolicy.swift): a tab
# out of sight for long enough lets go of its surfaces — renderers and the
# `keep` clients attached through them — and gets them back when it is shown,
# repainted by the daemon. The six tabs used last keep theirs.
#
# Nine tabs, the first split in two with its divider off the middle. First a
# run with hibernation off, for comparison: every tab visited, ten clients
# (one a pane), and the app's footprint. Then the run under test, visiting
# them the same way:
#   - six clients are left, one per tab kept; the tabs let go of are the three
#     least recently shown; the footprint is lower by what their surfaces cost;
#     every shell is still running and every tab still in the strip;
#   - a hibernated tab whose program asks to trust its folder is answered with
#     Enter through the daemon, without being woken;
#   - a hibernated tab shown again has a surface made for it, shows the screen
#     its shell left (its own lines, same shell) at the grid the daemon has, and
#     the tab used least recently is let go of in its place;
#   - the split tab comes back with both panes, both screens, and its divider
#     where it was left;
#   - going back and forth keeps the count, and the tab on screen keeps its
#     surface.
#
# Every tab runs a stand-in for Claude Code (a program named for a version
# number, which is how Keep tells) that prints two lines with its own pid,
# writes down what it is sent, and draws the trust question when asked to.
# Nothing is typed and no pointer moves: tabs are pressed and the divider is
# moved through the accessibility tree (tools/axpress.swift, tools/axsplit.swift),
# and the front is handed back to whatever had it.
#
#   tools/hibernate-test.sh              (SKIP_BUILD=1 to use the KeepDev built last;
#                                         KEEP_TEST_APP=<path to an .app> to drive another
#                                         build, under a name of its own)
#   tools/hibernate-test.sh --sabotage   the run under test with hibernation switched off
#                                        (KEEP_HYDRATED_LIMIT=off): its checks must fail
#
# Your app, its daemon and its settings are never touched.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

SABOTAGE=
[ "${1:-}" = "--sabotage" ] && SABOTAGE=1

APP=${KEEP_TEST_APP:-$(dev_app)}
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
SOCKET=/tmp/keep-hib-$$.sock
WORK=$(mktemp -d /tmp/keep-hib-XXXXXX)
AXPRESS=$WORK/axpress
AXSPLIT=$WORK/axsplit
KEEP=./target/release/keep
WS=hib
TABS=9
LIMIT=6
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
at_least() {  # at_least <what> <least> <got>
    if [ "$3" -ge "$2" ] 2>/dev/null; then
        say "  ok    $1 ($3, at least $2)"
        PASSED=$((PASSED + 1))
    else
        say "  FAIL  $1"
        say "        wanted at least: $2"
        say "        got:             $3"
        FAILED=$((FAILED + 1))
    fi
}

if [ -z "${KEEP_TEST_APP:-}" ] && [ -z "${SKIP_BUILD:-}" ]; then
    say "building $APP_NAME, the client and the daemon (yours is left alone)"
    ./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }
fi
swiftc -O tools/axpress.swift -o "$AXPRESS" 2>/dev/null || { say "could not build axpress"; exit 1; }
swiftc -O tools/axsplit.swift -o "$AXSPLIT" 2>/dev/null || { say "could not build axsplit"; exit 1; }

export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$WORK/state
export KEEP_STATUS_DIR=$WORK/status
export KEEP_HIB_WORK=$WORK
# What a surface shows a few seconds after it is made goes into the trace.
export KEEP_TRACE_SCREEN=1
mkdir -p "$KEEP_STATE_DIR" "$KEEP_STATUS_DIR" "$WORK/tabs"
require_scratch_socket
rm -f "$SOCKET"

# The tab's program: poses as Claude Code.
cat >"$WORK/printer" <<'PY'
#!/usr/bin/env python3
import os, select, sys, tty
WORK = os.environ["KEEP_HIB_WORK"]
me = os.getpid()
tty.setraw(0)
out = sys.stdout
def write(text):
    out.write(text)
    out.flush()
# Claude Code's mark in the title: with a version number for a name, that is
# how Keep tells it is Claude Code (Session.program(of:)).
write("\x1b]2;\u2733 teste\x07\x1b[2J\x1b[H")
write("MARK-%d first line of the shell\r\n" % me)
write("MARK-%d second line, further down\r\n" % me)
open(os.path.join(WORK, "tabs", str(me)), "w").close()
ask = os.path.join(WORK, "ask-%d" % me)
asked = False
raw = b""
while True:
    ready, _, _ = select.select([0], [], [], 0.2)
    if ready:
        data = os.read(0, 256)
        if not data:
            break
        raw += data
        with open(os.path.join(WORK, "in-%d" % me), "w") as f:
            f.write(raw.decode("utf-8", "replace").replace("\r", "<CR>"))
    if not asked and os.path.exists(ask):
        asked = True
        write("\x1b[2J\x1b[H\r\n Accessing workspace:\r\n\r\n /Users/thiago/projetos/novo\r\n\r\n")
        write(" Quick safety check: is this a project you created or one you trust?\r\n\r\n")
        write(" ❯ 1. Yes, I trust this folder\r\n   2. No, exit\r\n\r\n Enter to confirm · Esc to cancel\r\n")
PY
chmod +x "$WORK/printer"
cat >"$WORK/fake.c" <<C
#include <stdlib.h>
int main(void) { return system("$WORK/printer"); }
C
cc -o "$WORK/2.1.296" "$WORK/fake.c" || { say "could not build the stand-in"; exit 1; }

SHELL="$WORK/2.1.296" ./target/release/keepd >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
waited=0
while [ ! -S "$SOCKET" ] && [ "$waited" -lt 40 ]; do sleep 0.25; waited=$((waited + 1)); done

programs() { ls "$WORK/tabs" | wc -l | tr -d ' '; }
# The pid of the program that started since `programs` was <n>: the one name
# not in the list taken then. (By time it cannot be told: several start within
# a second, and a listing by time puts those in the order of their names.)
SEEN=
newest_program() {  # newest_program <count before>
    local waited=0
    while [ "$(programs)" -le "$1" ] && [ "$waited" -lt 80 ]; do sleep 0.25; waited=$((waited + 1)); done
    comm -13 <(printf '%s\n' $SEEN | sort) <(ls "$WORK/tabs" | sort) | head -1
    SEEN=$(ls "$WORK/tabs")
}

declare -a PID
for n in $(seq "$TABS"); do
    "$KEEP" new "$WS" >/dev/null 2>&1 || { say "could not open tab $n"; exit 1; }
    PID[$n]=$(newest_program $((n - 1)))
    SEEN=$(ls "$WORK/tabs")
done
[ -n "${PID[$TABS]:-}" ] || { say "the tabs' programs did not start"; exit 1; }

# --- helpers -----------------------------------------------------------------

SILENCE_S=30
ask() {  # ask <axpress arguments...>: its answer, or "(none)"
    local tries=0 got code start=$SECONDS
    while :; do
        got=$("$AXPRESS" "$APP_NAME" "$@" 2>/dev/null); code=$?
        [ "$code" -eq 0 ] && { printf '%s\n' "$got"; return; }
        if [ "$code" -eq 4 ]; then
            [ $((SECONDS - start)) -ge "$SILENCE_S" ] && break
        else
            tries=$((tries + 1)); [ "$tries" -ge 8 ] && break
        fi
        sleep 0.5
    done
    echo "(none)"
}
present() {  # present <label>: yes, no
    [ "$(ask frame "$1")" = "(none)" ] && echo no || echo yes
}
show() {  # show <tab number>: press it in the strip
    "$AXPRESS" "$APP_NAME" press "strip-tab-$WS/$1" >/dev/null 2>&1
}
divider() {  # the position of the divider between the panes on view, in points
    "$AXSPLIT" "$APP_NAME" list 2>/dev/null | awk -F'\t' 'NR == 1 { print $2 }'
}

# The processes under the app: a `login` and a `keep` client for every surface
# alive. (The programs in the tabs are the daemon's children, not the app's.)
under_app() {  # under_app <command name>
    local pid command n=0
    for pid in $(descendants "$APP_PID"); do
        command=$(ps -o comm= -p "$pid" 2>/dev/null) || continue
        [ "${command##*/}" = "$1" ] && n=$((n + 1))
    done
    echo "$n"
}
clients() { under_app keep; }
logins() { under_app login; }
clients_are() { [ "$(clients)" -eq "$1" ]; }
# The app's footprint in megabytes, as `footprint` reckons it (the RSS `ps`
# gives leaves out the graphics memory, which is most of what a surface costs):
# the middle of three readings a second apart.
footprint_mb() {
    local i
    for i in 1 2 3; do
        footprint "$APP_PID" 2>/dev/null | awk '/Footprint:/ {
            for (i = 1; i <= NF; i++) if ($i == "Footprint:") { v = $(i+1); u = $(i+2) }
            if (u == "KB") v = v / 1024; else if (u == "GB") v = v * 1024
            printf "%d\n", v; exit }'
        sleep 1
    done | sort -n | sed -n 2p
}
trace_count() {  # trace_count <extended regex>
    grep -acE "$1" "$WORK/app.log"
}
made() { trace_count "pool +made .* $WS/$1 "; }  # made <pane>: surfaces made for it so far
grid() {  # grid <pane>: the columns and rows the app last gave it
    grep -aoE "layout +$WS/$1 grid [0-9]+x[0-9]+" "$WORK/app.log" | tail -1 | awk '{print $NF}'
}
daemon_grid() {  # daemon_grid <tab>: the columns and rows the daemon has for it
    "$KEEP" ls 2>/dev/null | awk -v t="$1" '$1 == "tab" && $2 == t {
        for (i = 1; i <= NF; i++) if ($i ~ /^[0-9]+x[0-9]+$/) { print $i; exit } }'
}
screen_has() {  # screen_has <pane> <text>: yes when the last screen traced for it has it
    grep -aE "screen +$WS/$1 " "$WORK/app.log" | tail -1 | grep -qF -- "$2" && echo yes || echo no
}
alive() {  # alive <pid>: yes, no
    kill -0 "$1" 2>/dev/null && echo yes || echo no
}
wait_for() {  # wait_for <seconds> <command...>: succeeds as soon as the command does
    local limit=$1 start=$SECONDS
    shift
    while ! "$@" >/dev/null 2>&1; do
        [ $((SECONDS - start)) -ge "$limit" ] && return 1
        sleep 0.5
    done
}

FRONT=
launch() {  # launch [KEY=VALUE...]: started, settled, and put behind whatever was in front
    FRONT=$(osascript -e 'tell application "System Events" to get bundle identifier of first application process whose frontmost is true' 2>/dev/null)
    : >"$WORK/app.log"
    # Where the app was left is where it opens: the first tab, every time.
    cat >"$KEEP_STATE_DIR/windows.json" <<JSON
{"0":{"x":360,"y":160,"width":1100,"height":700,"workspaces":["$WS"],"tab":{"workspace":"$WS","root":1}}}
JSON
    env "$@" KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
    APP_PID=$!
    local waited=0
    while ! grep -aqE "sidebar +order" "$WORK/app.log" && [ "$waited" -lt 120 ]; do
        sleep 0.25; waited=$((waited + 1))
    done
    sleep 1.5
    if [ -n "$FRONT" ] && [ "$FRONT" != "missing value" ]; then
        osascript -e "tell application id \"$FRONT\" to activate" >/dev/null 2>&1
    fi
    # The window settles into its size before anything is measured.
    sleep 4
    local seen last=-1 still=0 settled=0
    while [ "$still" -lt 8 ] && [ "$settled" -lt 80 ]; do
        seen=$(trace_count "layout +[^ ]+ size [0-9]+x[0-9]+")
        if [ "$seen" = "$last" ]; then still=$((still + 1)); else still=0; last=$seen; fi
        sleep 0.25; settled=$((settled + 1))
    done
    if [ "$(present "strip-tab-$WS/1")" != yes ]; then
        say "the test's window cannot be seen: is another app in full screen? not run"
        exit 3
    fi
}
quit_app() {
    kill "$APP_PID" 2>/dev/null
    local waited=0
    while kill -0 "$APP_PID" 2>/dev/null && [ "$waited" -lt 40 ]; do sleep 0.25; waited=$((waited + 1)); done
    APP_PID=
}
visit_all() {  # every tab after the first in turn, a moment on each, ending on the last
    local n
    for n in $(seq 2 "$TABS"); do
        show "$n"
        sleep 1.5
    done
}

stop_app

# --- for comparison: nothing hibernated --------------------------------------

say "with hibernation off (KEEP_HYDRATED_LIMIT=off), for comparison"
launch KEEP_HYDRATED_LIMIT=off
ONE_MB=$(footprint_mb)
# The first tab split in two, as somebody would: File ▸ Split Right.
"$AXPRESS" "$APP_NAME" menu File "Split Right" >/dev/null 2>&1
SPLIT_PID=$(newest_program "$TABS")
SEEN=$(ls "$WORK/tabs")
PANE=$("$KEEP" ls 2>/dev/null | awk '$1 == "tab" && /\(pane of 1\)/ { print $2; exit }')
[ -n "$PANE" ] && [ -n "$SPLIT_PID" ] || { say "could not split the first tab"; exit 1; }
PID[$PANE]=$SPLIT_PID
sleep 2
visit_all
sleep 4
BASE_CLIENTS=$(clients)
BASE_MB=$(footprint_mb)
# What one surface costs, as near as two readings can say: nine full tabs
# against one (the split tab's two halves cost about one).
PER_TAB=$(( (BASE_MB - ONE_MB) / (TABS - 1) ))
say "  1 surface: ${ONE_MB} MB; every tab ($BASE_CLIENTS surfaces): ${BASE_MB} MB; about ${PER_TAB} MB a tab"
check "with hibernation off, every pane keeps its client" "$((TABS + 1))" "$BASE_CLIENTS"
quit_app
sleep 3

# --- the run under test ------------------------------------------------------

say ""
if [ -n "$SABOTAGE" ]; then
    say "sabotage: the run under test with hibernation switched off"
    launch KEEP_HYDRATED_LIMIT=off
else
    say "with hibernation ($LIMIT tabs kept)"
    launch
fi

# The divider off the middle: where it is put is where it must come back.
read -r HALF <<<"$(divider)"
[ -n "$HALF" ] && [ "$HALF" -gt 100 ] || { say "the split tab has no divider to move (got '$HALF')"; exit 1; }
"$AXSPLIT" "$APP_NAME" set 0 $((HALF * 7 / 10)) || { say "could not move the divider"; exit 1; }
sleep 2
DIVIDER=$(divider)
GRID_ROOT=$(grid 1)
GRID_PANE=$(grid "$PANE")
say "  split tab: divider at $DIVIDER pt (middle $HALF), panes $GRID_ROOT and $GRID_PANE"

visit_all
# A second after the last tab is shown, the ones out of sight are let go of.
wait_for 20 clients_are "$LIMIT"
sleep 2

say ""
say "after visiting $TABS tabs"
check "only $LIMIT clients are left, one per tab kept" "$LIMIT" "$(clients)"
check "a login in front of each, none left over" "$LIMIT" "$(logins)"
check "the panes let go of are those of the three tabs least recently shown" \
    "$(printf '%s\n' 1 "$PANE" 2 3 | sort -n | tr '\n' ' ')" \
    "$(grep -aoE "pool +hibernating w[0-9]+ $WS/[0-9]+" "$WORK/app.log" | sed 's#.*/##' | sort -n | tr '\n' ' ')"
MB=$(footprint_mb)
DROP=$((BASE_MB - MB))
say "  footprint ${MB} MB, against ${BASE_MB} MB with nothing hibernated: ${DROP} MB less"
# Two tabs' worth of the three let go of, and never less than 40 MB: a cost
# per tab read too low must not make this a check that cannot fail.
WANTED=$((2 * PER_TAB))
[ "$WANTED" -lt 40 ] && WANTED=40
at_least "the footprint is lower by at least two tabs' worth (MB)" "$WANTED" "$DROP"
LIVE=0
for n in $(seq "$TABS") "$PANE"; do [ "$(alive "${PID[$n]}")" = yes ] && LIVE=$((LIVE + 1)); done
check "every shell is still running" "$((TABS + 1))" "$LIVE"
check "the daemon still has every tab and the pane" "$((TABS + 1))" \
    "$("$KEEP" ls 2>/dev/null | grep -cE '^ +tab ')"
SHOWN=0
for n in $(seq "$TABS"); do [ "$(present "strip-tab-$WS/$n")" = yes ] && SHOWN=$((SHOWN + 1)); done
check "every tab is still in the strip" "$TABS" "$SHOWN"
check "the tab on screen is the last one shown" "selecionada" "$(ask value "strip-tab-$WS/$TABS")"

say ""
say "a hibernated tab asks to trust its folder"
BEFORE=$(made 3)
touch "$WORK/ask-${PID[3]}"
wait_for 25 grep -q "<CR>" "$WORK/in-${PID[3]}"
check "it is answered with Enter" "yes" \
    "$(grep -q '<CR>' "$WORK/in-${PID[3]}" 2>/dev/null && echo yes || echo no)"
check "through the daemon" "yes" \
    "$(grep -aqE "pool +$WS/3 is hibernated; typed through the daemon" "$WORK/app.log" && echo yes || echo no)"
check "and it was not woken to be" "$BEFORE" "$(made 3)"
check "no other tab was sent anything" "" "$(ls "$WORK" | grep '^in-' | grep -v "in-${PID[3]}$")"
check "still $LIMIT clients" "$LIMIT" "$(clients)"

say ""
say "back to a hibernated tab"
SCREENS=$(trace_count "screen +$WS/2 ")
show 2
screen_came() { [ "$(trace_count "screen +$WS/2 ")" -gt "$SCREENS" ]; }
wait_for 20 screen_came
wait_for 10 clients_are "$LIMIT"
sleep 1
check "a surface was made for it again" "2" "$(made 2)"
check "it is the tab on screen" "selecionada" "$(ask value "strip-tab-$WS/2")"
check "its screen is the one its shell left: the first line" yes "$(screen_has 2 "MARK-${PID[2]} first line")"
check "and the second" yes "$(screen_has 2 "MARK-${PID[2]} second line")"
check "the same shell is behind it" yes "$(alive "${PID[2]}")"
check "its grid is the one the daemon has for the tab" "$(daemon_grid 2)" "$(grid 2)"
check "and the one the tab shown before had, in the same window" "$(grid "$TABS")" "$(grid 2)"
check "the tab used least recently was let go of in its place" "4" \
    "$(grep -aoE "pool +hibernating w[0-9]+ $WS/[0-9]+" "$WORK/app.log" | tail -1 | sed 's#.*/##')"
check "so there are still $LIMIT clients" "$LIMIT" "$(clients)"
check "and a login in front of each" "$LIMIT" "$(logins)"

say ""
say "back to the split tab"
SCREENS=$(trace_count "screen +$WS/$PANE ")
show 1
screen_came() { [ "$(trace_count "screen +$WS/$PANE ")" -gt "$SCREENS" ]; }
wait_for 20 screen_came
# Its two panes back, and the tab used least recently (5) let go of.
wait_for 10 clients_are $((LIMIT + 1))
sleep 1
check "a surface was made again for each pane" "2 2" "$(made 1) $(made "$PANE")"
check "the first pane shows its shell's lines" yes "$(screen_has 1 "MARK-${PID[1]} first line")"
check "and so does the second" yes "$(screen_has "$PANE" "MARK-${PID[$PANE]} first line")"
check "the divider is where it was left" "$DIVIDER" "$(divider)"
check "and each pane has the grid it had" "$GRID_ROOT $GRID_PANE" "$(grid 1) $(grid "$PANE")"
check "the daemon agrees, for each pane" "$(daemon_grid 1) $(daemon_grid "$PANE")" "$(grid 1) $(grid "$PANE")"
check "the tab used least recently was let go of in its place" "5" \
    "$(grep -aoE "pool +hibernating w[0-9]+ $WS/[0-9]+" "$WORK/app.log" | tail -1 | sed 's#.*/##')"
check "six tabs kept: seven clients, two for the split tab" $((LIMIT + 1)) "$(clients)"

say ""
say "back and forth between two hibernated tabs"
for _ in 1 2 3; do
    show 3; sleep 1.2
    show 4; sleep 1.2
done
sleep 4
# Kept: 4, 3, 1 (two panes), 2, 9, 8.
check "six tabs kept: seven clients" $((LIMIT + 1)) "$(clients)"
LIVE=0
for n in $(seq "$TABS") "$PANE"; do [ "$(alive "${PID[$n]}")" = yes ] && LIVE=$((LIVE + 1)); done
check "every shell is still running" "$((TABS + 1))" "$LIVE"
check "the tab on screen keeps its surface" "yes" \
    "$([ "$(made 4)" -gt "$(trace_count "pool +hibernating w[0-9]+ $WS/4 ")" ] && echo yes || echo no)"
check "it is the tab on screen" "selecionada" "$(ask value "strip-tab-$WS/4")"
check "it shows its shell's lines" yes "$(screen_has 4 "MARK-${PID[4]} first line")"

quit_app
say ""
if [ -n "$SABOTAGE" ]; then
    say "sabotage: $FAILED check(s) failed, $PASSED passed"
    [ "$FAILED" -gt 0 ] && { say "caught"; exit 0; }
    say "NOT CAUGHT"
    exit 1
fi
say "passed $PASSED, failed $FAILED"
[ "$FAILED" -eq 0 ]
