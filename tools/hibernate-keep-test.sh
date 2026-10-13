#!/usr/bin/env bash
#
# Hibernation without losing anything (Display/Keeper.swift,
# crates/keep/src/keeper.rs): a tab whose surface is let go of is kept by its
# client, and the surface that comes back for it gets the whole terminal.
# tools/hibernate-test.sh proves the surfaces go and come back; this proves
# nothing is lost on the way, in the app, against a daemon of its own:
#
#   - the history: thousands of lines printed while the tab was shown, and
#     thousands more printed while it was hibernated, all there, once and in
#     order, when it is shown again — far more than the daemon keeps;
#   - a hibernated tab's program is answered as a surface answers it, asked
#     the same questions in the same tab shown and hibernated;
#   - the title a hibernated tab's program sets reaches the strip at once,
#     and the daemon still counts the tab as watched;
#   - Claude Code on the alternate screen: its screen as it left it while
#     hibernated, and the shell's screen and history under it when it goes;
#   - the zoom changed while tabs are hibernated: a tab shown again has the
#     grid the zoom gives, and so does its shell;
#   - ⌘P to a hibernated tab;
#   - a tab on screen in another window is never let go of;
#   - full screen: tabs are let go of and come back at the full-screen grid.
#
# Seven tabs, three kept (KEEP_HYDRATED_LIMIT=3). Each runs a stand-in for
# Claude Code (tools/hibernate-keep-test/program.py), told what to do through
# files. Tabs are shown and menus used through the accessibility tree; ⌘P is
# typed into the app's own window (tools/sendkey.swift); the front is handed
# back to whatever had it. Full screen takes the display for a few seconds.
#
#   tools/hibernate-keep-test.sh            (SKIP_BUILD=1 to use the KeepDev built last)
#   tools/hibernate-keep-test.sh --sabotage the same with a client from before keeping
#                                           (the installed Keep's, which reads only the first
#                                           three lines of the attach file) in a copy of the
#                                           build: the hibernation of before, which must fail
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
SOCKET=/tmp/keep-hk-$$.sock
WORK=$(mktemp -d /tmp/keep-hk-XXXXXX)
AXPRESS=$WORK/axpress
SENDKEY=$WORK/sendkey
FULLSCREEN=$WORK/axfullscreen
KEEP=./target/release/keep
WS=hk
TABS=7
LIMIT=3
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
    say "building $APP_NAME, the client and the daemon (yours is left alone)"
    ./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }
fi
swiftc -O tools/axpress.swift -o "$AXPRESS" 2>/dev/null || { say "could not build axpress"; exit 1; }
swiftc -O tools/sendkey.swift -o "$SENDKEY" 2>/dev/null || { say "could not build sendkey"; exit 1; }
swiftc -O tools/axfullscreen.swift -o "$FULLSCREEN" 2>/dev/null || { say "could not build axfullscreen"; exit 1; }
# A locked screen hides every window from the accessibility tree and keeps a
# window from going full screen: nothing here can be asked or done then.
cat >"$WORK/locked.swift" <<'SWIFT'
import CoreGraphics
let session = CGSessionCopyCurrentDictionary() as? [String: Any] ?? [:]
print((session["CGSSessionScreenIsLocked"] as? Bool) == true ? "locked" : "open")
SWIFT
swiftc -O "$WORK/locked.swift" -o "$WORK/locked" 2>/dev/null || { say "could not build the lock check"; exit 1; }
# Locked, what the accessibility tree says of the windows (the strip's text)
# and full screen are left out, and said to be; the rest runs, keys and menus
# reaching the app all the same. Only a run on an open screen is complete.
LOCKED=
if [ "$("$WORK/locked")" = locked ]; then
    LOCKED=1
    [ -n "${KEEP_TEST_COMPLETE:-}" ] && { say "the screen is locked and a complete run was asked for. not run"; exit 3; }
fi
skip() { say "  skip  $1 (the screen is locked)"; SKIPPED=$((SKIPPED + 1)); }
SKIPPED=0

export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$WORK/state
export KEEP_STATUS_DIR=$WORK/status
export KEEP_HIB_WORK=$WORK
export KEEP_TRACE_SCREEN=1
export KEEP_SCREEN_DUMP=$WORK/dump
# What each client did: kept, handed over, took over (crates/keep/src/keeper.rs).
export KEEP_CLIENT_LOG=$WORK/client.log
mkdir -p "$KEEP_STATE_DIR" "$KEEP_STATUS_DIR" "$WORK/tabs" "$WORK/dump"
require_scratch_socket
rm -f "$SOCKET"

cp tools/hibernate-keep-test/program.py "$WORK/program.py"
chmod +x "$WORK/program.py"
cat >"$WORK/fake.c" <<C
#include <stdlib.h>
int main(void) { return system("$WORK/program.py"); }
C
cc -o "$WORK/2.1.296" "$WORK/fake.c" || { say "could not build the stand-in"; exit 1; }

SHELL="$WORK/2.1.296" ./target/release/keepd >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
waited=0
while [ ! -S "$SOCKET" ] && [ "$waited" -lt 40 ]; do sleep 0.25; waited=$((waited + 1)); done

programs() { ls "$WORK/tabs" | wc -l | tr -d ' '; }
SEEN=
newest_program() {  # newest_program <count before>
    local waited=0
    while [ "$(programs)" -le "$1" ] && [ "$waited" -lt 120 ]; do sleep 0.25; waited=$((waited + 1)); done
    comm -13 <(printf '%s\n' $SEEN | sort) <(ls "$WORK/tabs" | sort) | head -1
    SEEN=$(ls "$WORK/tabs")
}
declare -a PID
for n in $(seq "$TABS"); do
    "$KEEP" new "$WS" >/dev/null 2>&1 || { say "could not open tab $n"; exit 1; }
    PID[$n]=$(newest_program $((n - 1)))
    # Said here: the line above ran in a subshell.
    SEEN=$(ls "$WORK/tabs")
done
[ -n "${PID[$TABS]:-}" ] || { say "the tabs' programs did not start"; exit 1; }

# --- helpers -----------------------------------------------------------------

wait_for() {  # wait_for <seconds> <command...>
    local limit=$1 start=$SECONDS
    shift
    while ! "$@" >/dev/null 2>&1; do
        [ $((SECONDS - start)) -ge "$limit" ] && return 1
        sleep 0.5
    done
}
# Have tab <n>'s program do these, and wait until it has.
tell() {  # tell <tab> <command>...
    local pid=${PID[$1]}
    shift
    rm -f "$WORK/done-$pid"
    printf '%s\n' "$@" >"$WORK/cmd-$pid.tmp"
    mv "$WORK/cmd-$pid.tmp" "$WORK/cmd-$pid"
    wait_for 60 test -f "$WORK/done-$pid" || say "  (tab $1's program did not answer)"
}
# Print <count> lines in tab <n> a few hundred at a time, each lot seen on
# its surface before the next is printed. A program that pours thousands of
# lines out at once can outrun a surface: the daemon stops queueing for a
# client that far behind and repaints it from its own page or two, and what
# fell off that page in between is gone from the surface too — the daemon's
# own limit (crates/keepd/src/tab.rs, CLIENT_BACKLOG), the same whether or
# not anything is hibernated, and not what this test is about.
print_seen() {  # print_seen <tab> <count> <first line number>
    local tab=$1 count=$2 from=$3 lot
    while [ "$count" -gt 0 ]; do
        lot=$(( count < 250 ? count : 250 ))
        tell "$tab" "print $lot"
        from=$((from + lot)); count=$((count - lot))
        wait_for 30 dump_has "$tab" "$(printf 'P%s-L%07d' "${PID[$tab]}" $((from - 1)))"
    done
}
# The same, into a tab kept by its client: no surface to see it on, a moment
# between lots instead.
print_kept() {  # print_kept <tab> <count>
    local count=$2 lot
    while [ "$count" -gt 0 ]; do
        lot=$(( count < 250 ? count : 250 ))
        tell "$1" "print $lot"
        count=$((count - lot))
        sleep 0.5
    done
}
SILENCE_S=30
ask_ax() {  # ask_ax <axpress arguments...>
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
present() { [ "$(ask_ax frame "$1")" = "(none)" ] && echo no || echo yes; }
# ⌘1 to ⌘8, by key code: Show Tab <n>, in the strip's order, which is the tabs'.
KEYCODE=(0 18 19 20 21 23 22 26 28)
show() {  # show <tab>: ⌘<tab>, then a press in the strip, until it is the tab on screen
    local tries=0
    while [ "$tries" -lt 5 ]; do
        if [ $((tries % 2)) -eq 0 ]; then
            "$SENDKEY" "$APP_PID" key "${KEYCODE[$1]}" cmd >/dev/null 2>&1
        else
            "$AXPRESS" "$APP_NAME" press "strip-tab-$WS/$1" >/dev/null 2>&1
        fi
        wait_for 6 shown "$1" && return 0
        tries=$((tries + 1))
    done
    return 1
}
shown() {  # shown <tab>: the trace says it was put on screen, last
    grep -aoE "switch +→ $WS/[0-9]+" "$WORK/app.log" | tail -1 | grep -qE "/$1\$"
}
trace_count() { grep -acE "$1" "$WORK/app.log"; }
made() { trace_count "pool +made .* $WS/$1 "; }
hibernated() { trace_count "pool +hibernating w[0-9]+ $WS/$1 kept=true"; }
taken() { trace_count "keeper +$WS/$1 takes over "; }
# How many clients said they took a tab over (KEEP_CLIENT_LOG): the app
# naming a keeper is not the client getting the tab from it.
took() { local n; n=$(grep -c ' took over ' "$WORK/client.log" 2>/dev/null); echo "${n:-0}"; }
grid() { grep -aoE "layout +$WS/$1 grid [0-9]+x[0-9]+" "$WORK/app.log" | tail -1 | awk '{print $NF}'; }
daemon_grid() {
    "$KEEP" ls 2>/dev/null | awk -v t="$1" '$1 == "tab" && $2 == t {
        for (i = 1; i <= NF; i++) if ($i ~ /^[0-9]+x[0-9]+$/) { print $i; exit } }'
}
watched() {  # watched <tab>: what the daemon says of it (attached, attached ×2, idle, running)
    "$KEEP" ls 2>/dev/null | awk -v t="$1" '$1 == "tab" && $2 == t {
        if ($NF ~ /^\xc3\x97/ || $NF ~ /^×/) print $(NF-1) " " $NF; else print $NF; exit }'
}
dump() { cat "$WORK/dump/$WS-$1.txt" 2>/dev/null; }
dump_has() { dump "$1" | grep -qF -- "$2"; }
# The numbered lines of tab <n>'s program in what its surface holds: "yes"
# when they are 1 to <count>, each once and in order.
lines_whole() {  # lines_whole <tab> <count>
    dump "$1" | grep -aoE "P${PID[$1]}-L[0-9]{7}" | sed 's/.*-L//' | awk -v want="$2" '
        { n++; if ($1 + 0 != n) bad = 1 }
        END { print (!bad && n == want) ? "yes" : "no (" n " lines)" }'
}
under_app() {  # under_app <command name>: processes of that name under the app
    local pid command n=0
    for pid in $(descendants "$APP_PID"); do
        command=$(ps -o comm= -p "$pid" 2>/dev/null) || continue
        [ "${command##*/}" = "$1" ] && n=$((n + 1))
    done
    echo "$n"
}
clients() { under_app keep; }

FRONT=
launch() {  # launch [KEY=VALUE...]
    FRONT=$(osascript -e 'tell application "System Events" to get bundle identifier of first application process whose frontmost is true' 2>/dev/null)
    : >"$WORK/app.log"
    cat >"$KEEP_STATE_DIR/windows.json" <<JSON
{"0":{"x":360,"y":160,"width":1100,"height":700,"workspaces":["$WS"],"tab":{"workspace":"$WS","root":1}}}
JSON
    env "$@" KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
    APP_PID=$!
    local waited=0
    while ! grep -aqE "sidebar +order" "$WORK/app.log" && [ "$waited" -lt 160 ]; do
        sleep 0.25; waited=$((waited + 1))
    done
    sleep 1.5
    give_back_front
    sleep 4
    if [ -z "$LOCKED" ] && [ "$(present "strip-tab-$WS/1")" != yes ]; then
        say "the test's window cannot be seen: is another app in full screen? not run"
        exit 3
    fi
}
give_back_front() {
    if [ -n "$FRONT" ] && [ "$FRONT" != "missing value" ]; then
        osascript -e "tell application id \"$FRONT\" to activate" >/dev/null 2>&1
    fi
}

stop_app
if [ -n "$SABOTAGE" ]; then
    OLD_CLIENT=${KEEP_OLD_CLIENT:-/Applications/Keep.app/Contents/Resources/keep}
    [ -x "$OLD_CLIENT" ] || { say "no client from before keeping at $OLD_CLIENT"; exit 1; }
    if strings "$OLD_CLIENT" 2>/dev/null | grep -q keeper.sock; then
        say "the client at $OLD_CLIENT keeps tabs already: no sabotage"; exit 1
    fi
    say "sabotage: a client from before keeping ($OLD_CLIENT)"
    mkdir -p "$WORK/sabotaged"
    cp -R "$APP" "$WORK/sabotaged/"
    APP=$WORK/sabotaged/$(basename "$APP")
    BIN=$APP/Contents/MacOS/$APP_NAME
    cp "$OLD_CLIENT" "$APP/Contents/Resources/keep"
    codesign --force --sign - "$APP/Contents/Resources/keep" >/dev/null 2>&1
    codesign --force --deep --sign - "$APP" >/dev/null 2>&1
fi
launch KEEP_HYDRATED_LIMIT=$LIMIT

# --- the history, and a program's questions, through hibernation -------------

say ""
say "a tab's history and its program's answers, through hibernation"
tell 1 "title tab-1"
wait_for 30 test -s "$WORK/dump/$WS-1.txt"
print_seen 1 3000 1
check "tab 1 shows the 3000 lines it printed" "yes" "$(lines_whole 1 3000)"
show 2 || say "  (tab 2 would not come up)"
tell 2 "title tab-2" "print 50"
show 3 || say "  (tab 3 would not come up)"
tell 3 "title tab-3" "print 200" "claude on" "claude count 1"
wait_for 30 dump_has 3 "count 1"
# Tab 2's program asks its questions with its surface still there, out of
# sight: three tabs, the limit, and nothing let go of yet.
tell 2 "ask"
for n in 4 5 6 7; do
    show "$n" || say "  (tab $n would not come up)"
    tell "$n" "title tab-$n" "print 20"
done
# A second after the last switch the tabs out of sight are let go of.
wait_for 30 test "$(hibernated 3)" -ge 1
sleep 2
check "the three tabs used least recently were let go of, kept by their clients" "1 1 1" \
    "$(hibernated 1) $(hibernated 2) $(hibernated 3)"
check "only $LIMIT clients are left under the app" "$LIMIT" "$(clients)"
check "the daemon still counts a hibernated tab as watched" "attached" "$(watched 1)"

say ""
say "while they are hibernated"
print_kept 1 2000
tell 1 "title renamed-while-hibernated"
tell 2 "ask"
tell 3 "claude count 7"
wait_for 20 grep -aqE "keeper .* title: renamed-while-hibernated" "$WORK/app.log"
check "the title set while hibernated reached the app at once" "yes" \
    "$(grep -aqE "keeper .* title: renamed-while-hibernated" "$WORK/app.log" && echo yes || echo no)"
if [ -n "$LOCKED" ]; then
    skip "and the strip"
else
    wait_for 20 test "$(ask_ax value "strip-title-$WS/1")" = "renamed-while-hibernated"
    check "and the strip" "renamed-while-hibernated" "$(ask_ax value "strip-title-$WS/1")"
fi
SHOWN_ANSWERS=$(cat "$WORK/answers-${PID[2]}-1" 2>/dev/null)
KEPT_ANSWERS=$(cat "$WORK/answers-${PID[2]}-2" 2>/dev/null)
QUESTIONS=$(printf '%s\n' "$SHOWN_ANSWERS" | grep -c .)
check "its program was answered as the surface had answered it ($QUESTIONS questions)" "$SHOWN_ANSWERS" "$KEPT_ANSWERS"
check "and was answered at all" "yes" \
    "$(printf '%s\n' "$KEPT_ANSWERS" | grep -qF '?62;22;52c' && echo yes || echo no)"
check "no surface was made for them meanwhile" "1 1 1" "$(made 1) $(made 2) $(made 3)"

say ""
say "back to the tab with the long history"
TOOK=$(took)
show 1 || say "  (tab 1 would not come up)"
wait_for 60 dump_has 1 "P${PID[1]}-L0005000"
sleep 1.5
check "a surface was made again, taking the tab over from its client" "2 1 $((TOOK + 1))" \
    "$(made 1) $(taken 1) $(took)"
check "every line is there, once and in order: 3000 shown, 2000 hibernated" "yes" "$(lines_whole 1 5000)"
check "its grid is the daemon's" "$(daemon_grid 1)" "$(grid 1)"

say ""
say "back to Claude Code on the alternate screen"
show 3 || say "  (tab 3 would not come up)"
wait_for 30 dump_has 3 "count 7"
check "it is as the program left it while hibernated" "yes" \
    "$(dump 3 | grep -qF 'count 7' && dump 3 | grep -qF 'Claude Code' && echo yes || echo no)"
tell 3 "claude off"
wait_for 30 dump_has 3 "P${PID[3]}-L0000200"
sleep 1.5
check "when it leaves, the shell's 200 lines are under it" "yes" "$(lines_whole 3 200)"

# --- the zoom ------------------------------------------------------------------

say ""
say "the zoom, changed while tabs are hibernated"
# Kept now: 3, 1, 7. Hibernated: 2, 4, 5, 6. The tab on screen takes the
# zoom at once, and its client tells the daemon: the grid read there.
BEFORE=$(daemon_grid 3)
"$AXPRESS" "$APP_NAME" menu View "Zoom In" >/dev/null 2>&1
wait_for 20 test "$(daemon_grid 3)" != "$BEFORE"
sleep 2
ZOOMED=$(daemon_grid 3)
check "the zoom changed the grid of the tab on screen" "yes" "$([ "$ZOOMED" != "$BEFORE" ] && echo yes || echo no)"
show 2 || say "  (tab 2 would not come up)"
wait_for 30 dump_has 2 "P${PID[2]}-L0000050"
sleep 3
check "a hibernated tab shown after it has the zoom's grid" "$ZOOMED" "$(grid 2)"
check "and so does its shell" "$ZOOMED" "$(daemon_grid 2)"
check "with its lines" "yes" "$(lines_whole 2 50)"
"$AXPRESS" "$APP_NAME" menu View "Actual Size" >/dev/null 2>&1
sleep 3

# --- ⌘P ---------------------------------------------------------------------------

say ""
say "⌘P to a hibernated tab"
# Kept now: 2, 3, 1. Hibernated: 4, 5, 6, 7.
MADE4=$(made 4)
TOOK=$(took)
"$SENDKEY" "$APP_PID" key 35 cmd >/dev/null 2>&1
sleep 2
"$SENDKEY" "$APP_PID" text "tab-4" >/dev/null 2>&1
sleep 1.5
"$SENDKEY" "$APP_PID" key 36 >/dev/null 2>&1
wait_for 20 shown 4
wait_for 30 dump_has 4 "P${PID[4]}-L0000020"
sleep 1.5
check "it is the tab on screen" "yes" "$(shown 4 && echo yes || echo no)"
check "taken over from its client" "$((MADE4 + 1)) 1 $((TOOK + 1))" "$(made 4) $(taken 4) $(took)"
check "with its lines" "yes" "$(lines_whole 4 20)"
give_back_front

# --- another window ------------------------------------------------------------------

say ""
say "a tab on screen in another window"
if [ -n "$LOCKED" ]; then
    # Behind the lock the app cannot be put in front, and with it behind, a
    # key the menu answers goes to one window and the return after it to
    # the other.
    skip "a second window: the app cannot be put in front behind the lock"
else
# The first window shows tab 2; a second window, made now, is shown the tab
# the first one shows (⌘P and return, as tools/windows-test.sh does) and
# then goes on through three others. In the second window tab 2 is soon the
# tab used least recently — but it is on screen in the first, and is not let
# go of.
show 2 || say "  (tab 2 would not come up)"
HIB2=$(trace_count "pool +hibernating w[0-9]+ $WS/2 ")
# In front for this: with the app behind, a key the menu answers (⌘P) goes
# to one window and the return after it to the other.
osascript -e "tell application \"System Events\" to set frontmost of process \"$APP_NAME\" to true" \
    >/dev/null 2>&1
sleep 1
"$SENDKEY" "$APP_PID" key 45 cmd shift >/dev/null 2>&1        # ⌘⇧N
sleep 3
"$SENDKEY" "$APP_PID" key 35 cmd >/dev/null 2>&1              # ⌘P
sleep 2
"$SENDKEY" "$APP_PID" key 36 >/dev/null 2>&1                  # return
wait_for 20 test "$(watched 2 | tr -d ' ')" = "attached×2"
check "the daemon counts two viewers of tab 2" "attached×2" "$(watched 2 | tr -d ' ')"
SECOND=$(grep -aoE "pool +made (w[0-9]+) $WS/2 " "$WORK/app.log" | tail -1 | awk '{print $3}')
for n in 5 6 7; do
    show "$n" || say "  (tab $n would not come up in the second window)"
    sleep 1
done
sleep 4
check "the second window went on through three other tabs" "yes" \
    "$(grep -aqE "switch +→ $WS/7 from=$WS/6" "$WORK/app.log" && echo yes || echo no)"
check "neither window let go of tab 2 while the first shows it" "$HIB2" \
    "$(trace_count "pool +hibernating w[0-9]+ $WS/2 ")"
check "while tabs out of sight in the second window were let go of" "yes" \
    "$([ -n "$SECOND" ] && grep -aqE "pool +hibernating $SECOND $WS/[56] " "$WORK/app.log" && echo yes || echo no)"
"$SENDKEY" "$APP_PID" key 13 cmd alt >/dev/null 2>&1          # ⌥⌘W
sleep 3
give_back_front
fi

# --- full screen -------------------------------------------------------------------------

say ""
say "full screen"
if [ -n "$LOCKED" ]; then
    skip "full screen: a window does not go full screen behind the lock"
elif [ "$("$FULLSCREEN" "$APP_NAME" state 2>/dev/null)" = "" ]; then
    say "  skip  the window cannot be put in full screen from here"
else
    WINDOWED=$(grid 7)
    "$FULLSCREEN" "$APP_NAME" on >/dev/null 2>&1
    wait_for 20 test "$("$FULLSCREEN" "$APP_NAME" state 2>/dev/null)" = 1
    sleep 4
    FULL=$(grep -aoE "layout +$WS/[0-9]+ grid [0-9]+x[0-9]+" "$WORK/app.log" | tail -1 | awk '{print $NF}')
    check "the window is in full screen" "1" "$("$FULLSCREEN" "$APP_NAME" state 2>/dev/null)"
    check "with a bigger grid" "yes" "$([ "$FULL" != "$WINDOWED" ] && echo yes || echo no)"
    # Hibernated now, among others: 1. Shown in full screen:
    HIB_BEFORE=$(trace_count "pool +hibernating w[0-9]+ $WS/")
    show 1 || say "  (tab 1 would not come up)"
    sleep 4
    check "a hibernated tab comes back at the full-screen grid" "$FULL" "$(grid 1)"
    check "and so does its shell" "$FULL" "$(daemon_grid 1)"
    check "with all its history" "yes" "$(lines_whole 1 5000)"
    for n in 3 4 5; do
        show "$n" || say "  (tab $n would not come up)"
        sleep 1
    done
    sleep 3
    check "tabs are let go of in full screen too" "yes" \
        "$([ "$(trace_count "pool +hibernating w[0-9]+ $WS/")" -gt "$HIB_BEFORE" ] && echo yes || echo no)"
    check "still $LIMIT clients under the app" "$LIMIT" "$(clients)"
    "$FULLSCREEN" "$APP_NAME" off >/dev/null 2>&1
    wait_for 20 test "$("$FULLSCREEN" "$APP_NAME" state 2>/dev/null)" = 0
    sleep 3
fi
give_back_front

LIVE=0
for n in $(seq "$TABS"); do kill -0 "${PID[$n]}" 2>/dev/null && LIVE=$((LIVE + 1)); done
check "every tab's program is still running" "$TABS" "$LIVE"

kill "$APP_PID" 2>/dev/null
wait_for 10 sh -c "! kill -0 $APP_PID"
APP_PID=
sleep 2
KEEPERS=$("$KEEP" ls 2>/dev/null | grep -cE '^ +tab .* attached')
check "with the app gone, its clients let go of every tab" "0" "$KEEPERS"

if [ "$FAILED" -gt 0 ] && [ -z "$SABOTAGE" ]; then
    say ""
    say "what the clients did:"
    sed 's/^/  /' "$WORK/client.log" 2>/dev/null | tail -40
fi

say ""
if [ -n "$SABOTAGE" ]; then
    say "sabotage: $FAILED check(s) failed, $PASSED passed"
    [ "$FAILED" -gt 0 ] && { say "caught"; exit 0; }
    say "NOT CAUGHT"
    exit 1
fi
say "passed $PASSED, failed $FAILED${SKIPPED:+, skipped $SKIPPED}"
[ "$FAILED" -eq 0 ]
