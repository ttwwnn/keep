#!/usr/bin/env bash
#
# A conversation left alone leaves memory, its tab stays, and it comes back
# when the tab is shown (keep restauro adormecer / acordar, 21/10/2026).
#
# A daemon of its own holds three tabs of one workspace: a shell, and two
# running the AI stand-in of keep-ia as `claude --effort max`, both
# conversations left alone for two hours against a limit of one
# (KEEP_ADORMECER_PARADA_S). KeepDev opens on it, and the second tab is put on
# screen (Window > Show Next Tab, through the accessibility tree: no keys, no
# mouse). The app's first round of putting conversations to sleep, 20 s after
# it opens, closes the third's — never the one on screen — and leaves that tab
# at its shell. Then the third tab is shown, and its conversation comes back
# in it, with the options it had.
#
#   tools/adormecer-test.sh
#
# At the end, a tab's zzz button (axpress press sleep-Alfa/2) does the same
# for that tab without waiting for the twelve hours.
#
# To see it fail: take `TabRestore.wake(...)` out of
# TabContentContainer.markVisible, and the last two checks fail.
#
# Your app, your daemon and your conversations are never touched: a scratch
# socket, KEEP_STATE_DIR and a made-up home in a scratch folder.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=$(dev_app)
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
WORK=$(mktemp -d /tmp/keep-adormecer-XXXXXX)
SOCKET=/tmp/ka-$$.sock
KEEP=$PWD/target/release/keep
WS=Alfa
PASSED=0
FAILED=0

cleanup() {
    [ -n "${APP_PID:-}" ] && kill -9 "$APP_PID" 2>/dev/null
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
    # The stand-ins of this run outlive their daemon: theirs only, by the folder they were given.
    for pid in $(pgrep -f "claude --effort max"); do
        ps eww -o command= -p "$pid" 2>/dev/null | grep -qF "$WORK" && kill "$pid" 2>/dev/null
    done
    [ -n "${KEEP_WORK:-}" ] && printf 'kept: %s\n' "$WORK" || rm -rf "$WORK"
    rm -f "$SOCKET"
}
trap cleanup EXIT INT TERM

say() { printf '%s\n' "$*"; }
check() {  # check <what> <wanted> <got>
    if [ "$3" = "$2" ]; then say "  ok    $1"; PASSED=$((PASSED + 1))
    else say "  FAIL  $1"; say "        wanted: $2"; say "        got:    $3"; FAILED=$((FAILED + 1)); fi
}

# What runs in front in each tab of the workspace: "<tab>=<command>", by tab.
fronts() {
    "$KEEP" ia abas | python3 -c '
import json, sys
d = json.load(sys.stdin)
print(";".join(sorted("%s=claude" % i["aba"] for i in d.get("ia", []) if i["workspace"] == "'"$WS"'")))'
}
# The command line of the stand-in in a tab.
argv_of() {
    local pid
    pid=$("$KEEP" ia abas | python3 -c '
import json, sys
print(next((i["pid"] for i in json.load(sys.stdin).get("ia", []) if i["workspace"] == "'"$WS"'" and i["aba"] == '"$1"'), 0))')
    [ "$pid" != 0 ] && ps -o command= -p "$pid"
}

say "building $APP_NAME, the client, the daemon, the AI stand-in and axpress (yours is left alone)"
./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }
cargo build -q -p keep-ia --examples 2>/dev/null || { say "could not build the AI stand-in"; exit 1; }
swiftc -O tools/axpress.swift -o "$WORK/axpress" 2>/dev/null || { say "could not build axpress"; exit 1; }
swiftc -O tools/mousedrag.swift -o "$WORK/mousedrag" 2>/dev/null || { say "could not build mousedrag"; exit 1; }

mkdir -p "$WORK/bin" "$WORK/casa/.claude" "$WORK/um" "$WORK/dois" "$WORK/state"
cp target/debug/examples/ia_falsa "$WORK/bin/claude"
export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$WORK/state
export KEEP_RESTAURO_ESTADO=$KEEP_STATE_DIR/restauro
export KEEP_AI_USAGE_HOME=$WORK/casa
export KEEP_IA_HOME=$WORK/casa
export KEEP_IA_ESTADO=$WORK/casa/.keep-ia-estado
export KEEP_IA_CLAUDE_BIN=$WORK/bin/claude
export KEEP_ADORMECER_PARADA_S=3600
unset CLAUDE_SECURESTORAGE_CONFIG_DIR CLAUDE_CONFIG_DIR KEEP_IA_ESCOLHA CLAUDE_KIT_CONTA CODEX_HOME
require_scratch_socket

# The tabs run /bin/sh with the stand-in first on the PATH: nobody's shell
# function or real Claude Code in them.
rm -f "$SOCKET"
SHELL=/bin/sh PS1='$ ' PATH="$WORK/bin:$PATH" ./target/release/keepd >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
waited=0
while [ ! -S "$SOCKET" ] && [ "$waited" -lt 40 ]; do sleep 0.25; waited=$((waited + 1)); done
(cd "$WORK" && "$KEEP" new "$WS" >/dev/null)
for folder in um dois; do
    "$KEEP" nova --workspace "$WS" --pasta "$WORK/$folder" --argumentos -- --effort max >/dev/null 2>&1
done
waited=0
while [ "$(fronts)" != "2=claude;3=claude" ] && [ "$waited" -lt 40 ]; do sleep 0.5; waited=$((waited + 1)); done
check "two conversations of the stand-in" "2=claude;3=claude" "$(fronts)"
# Left alone for two hours (the stand-in's turns carry no time: the file's counts).
touch -t "$(date -v-2H +%Y%m%d%H%M)" "$WORK"/casa/.claude/projects/*/*.jsonl

say "starting $APP_NAME on its own daemon"
stop_app
KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
APP_PID=$!
sleep 6
place_on_screen
"$WORK/axpress" "$APP_NAME" menu Window "Show Next Tab" >/dev/null 2>&1
sleep 1
check "the second tab is on screen" "yes" "$(grep -q "switch    → $WS/2 " "$WORK/app.log" && echo yes || echo no)"

# The first round is 20 s after opening; a conversation leaves in a few more.
waited=0
while [ "$(fronts)" = "2=claude;3=claude" ] && [ "$waited" -lt 90 ]; do sleep 1; waited=$((waited + 1)); done
check "the conversation out of sight left memory, the one on screen did not" "2=claude" "$(fronts)"
ASLEEP=3
check "its tab is still there, at its shell" "yes" "$("$KEEP" ls 2>/dev/null | grep -q "tab $ASLEEP " && echo yes || echo no)"
ID=$(python3 -c "
import json; d = json.load(open('$KEEP_RESTAURO_ESTADO/adormecidas.json'))
print(','.join(a['ia']['conversa'] for a in d['abas'] if a['aba'] == $ASLEEP))" 2>/dev/null)
check "the conversation is written down to come back" "36" "${#ID}"

# Shown after the round is over: a tab put on screen while the round still
# runs is woken at its end, which is another path than showing it.
waited=0
while ! grep -q '"adormecidas":1' "$WORK/app.log" && [ "$waited" -lt 30 ]; do sleep 1; waited=$((waited + 1)); done
sleep 2
say "showing the tab put to sleep"
"$WORK/axpress" "$APP_NAME" menu Window "Show Next Tab" >/dev/null 2>&1
waited=0
while [ "$(fronts)" != "2=claude;3=claude" ] && [ "$waited" -lt 60 ]; do sleep 1; waited=$((waited + 1)); done
check "its conversation came back as it was shown" "2=claude;3=claude" "$(fronts)"
check "the same conversation, with the options it had" "yes" \
    "$(argv_of "$ASLEEP" | grep -qF -- "--effort max --resume $ID" && echo yes || echo no)"

# The zzz of a tab, without the twelve hours: the conversation of tab 2, left
# alone a moment ago, leaves memory; the one on screen (3) stays.
touch "$WORK"/casa/.claude/projects/*/*.jsonl
say "pressing the zzz of tab 2"
# The zzz shows under the pointer only: park it on the row of tab 2 and press
# once it is there (rc 4: the app did not answer in time, ask again).
for _ in 1 2 3 4 5 6; do
    # In front: a hover does not reach a window another app covers.
    osascript -e "tell application \"System Events\" to set frontmost of process \"$APP_NAME\" to true" >/dev/null 2>&1
    read -r RX RY RW RH < <("$WORK/axpress" "$APP_NAME" frame "sidebar-title-$WS/2")
    [ -n "${RX:-}" ] && "$WORK/mousedrag" move $((RX + RW / 2)) $((RY + RH / 2))
    sleep 1
    "$WORK/axpress" "$APP_NAME" press "sleep-$WS/2" >/dev/null 2>&1 && break
done
waited=0
while [ "$(fronts)" = "2=claude;3=claude" ] && [ "$waited" -lt 30 ]; do sleep 1; waited=$((waited + 1)); done
check "the zzz put tab 2 to sleep, not the one on screen" "$ASLEEP=claude" "$(fronts)"
check "and wrote it down to come back" "yes" \
    "$(python3 -c "
import json; d = json.load(open('$KEEP_RESTAURO_ESTADO/adormecidas.json'))
print('yes' if any(a['aba'] == 2 for a in d['abas']) else 'no')" 2>/dev/null)"

say "$PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
