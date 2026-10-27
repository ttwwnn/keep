#!/usr/bin/env bash
#
# Closing a tab whose conversation made a worktree, clicked through in the app.
#
# tools/worktrees-test.sh proves Worktrees.swift on its own; this drives the
# whole of it in KeepDev: File > Close Tab, the question the app puts up with
# the worktree listed in it, and the two answers — Cancelar, after which
# nothing may have moved and the helper may not have been asked to prepare
# anything, and Fechar Aba, after which the worktree is in the Trash and the
# helper was asked to prepare it before and to conclude after. In between, a
# conversation that made forty: the question still fits on the screen, both
# its buttons on it, the last of the forty in it, and escape puts it away —
# one that listed twenty-seven came out taller than the screen, its buttons
# below the edge, and escape did nothing, so nothing could answer it. Return
# answers no button: the ending is destructive, and AppKit keeps return off it.
#
# The core is the fake one (tools/worktrees-test/fake-helper.py), called as
# the app's `keep` (`worktrees` first): which worktrees a conversation made is
# the core's to decide, and the core's tests prove it. The worktree is a real
# one, in a repository made for the test, and goes to the real Trash under a
# name no one else uses; it is taken back out at the end.
#
#   tools/close-trash-test.sh          (SKIP_BUILD=1 to use the KeepDev built last)
#
# No mouse: the menu item and the buttons are pressed through the
# accessibility tree (tools/axpress.swift), the question read the same way
# (tools/axtext.swift), and escape is posted to the app's pid
# (tools/sendkey.swift), front or not. Your app and your daemon are never
# touched.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=$(dev_app)
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
SOCKET=/tmp/keep-closetrash-$$.sock
WORK=$(mktemp -d /tmp/keep-closetrash-XXXXXX)
AXTEXT=$WORK/axtext
AXPRESS=$WORK/axpress
SENDKEY=$WORK/sendkey
PASSED=0
FAILED=0
WT_NAME=keep-closetrash-wt-$$
WT=$WORK/$WT_NAME

cleanup() {
    [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
    rm -rf "$HOME/.Trash/$WT_NAME"
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

if [ -z "${SKIP_BUILD:-}" ]; then
    say "building $APP_NAME (yours is left alone)"
    ./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }
fi
swiftc -O tools/axtext.swift -o "$AXTEXT" 2>/dev/null || { say "could not build axtext"; exit 1; }
swiftc -O tools/axpress.swift -o "$AXPRESS" 2>/dev/null || { say "could not build axpress"; exit 1; }
swiftc -O tools/sendkey.swift -o "$SENDKEY" 2>/dev/null || { say "could not build sendkey"; exit 1; }

# A repository of the test's own, and a worktree of it on a branch.
git init -q "$WORK/repo" && git -C "$WORK/repo" commit -q --allow-empty -m base \
    && git -C "$WORK/repo" worktree add -q -b ramo-do-teste "$WT" \
    || { say "could not make the test repository"; exit 1; }
echo "trabalho" >"$WT/arquivo.txt"

# The fake helper answers `listar` with that worktree.
cp tools/worktrees-test/fake-helper.py "$WORK/"
chmod +x "$WORK/fake-helper.py"
export KEEP_WORKTREES_BIN=$WORK/fake-helper.py
export FALSO_DIR=$WORK
python3 - "$WORK/listar.json" "$WT" <<'PY'
import json, sys
json.dump({"versao": 1, "abas": [{"alvo": "w:1", "pids": []}],
           "lixeira": [{"caminho": sys.argv[2], "ramo": "ramo-do-teste", "head": "abc1234",
                        "alteracoes": 1, "commits_so_aqui": 0, "processos": []}],
           "mantidas": [], "avisos": []}, open(sys.argv[1], "w"))
PY

export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$WORK/state
mkdir -p "$KEEP_STATE_DIR"
require_scratch_socket
rm -f "$SOCKET"
"$APP/Contents/Resources/keepd" >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
waited=0
while [ ! -S "$SOCKET" ] && [ "$waited" -lt 40 ]; do sleep 0.25; waited=$((waited + 1)); done

stop_app
KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
APP_PID=$!
waited=0
while ! grep -aqE "sidebar +order" "$WORK/app.log" && [ "$waited" -lt 120 ]; do sleep 0.25; waited=$((waited + 1)); done
place_on_screen
sleep 2

asked() {  # the helper's calls, by their first word — but `indexar`, which the app asks on its own
    python3 -c 'import json,sys
try: print(" ".join(a for a in (json.loads(l)["argv"][0] for l in open(sys.argv[1])) if a != "indexar"))
except FileNotFoundError: print("")' "$WORK/chamadas.log"
}
question_up() {  # question_up [words]: wait for the question saying them
    local words=${1:-vai para a lixeira} waited=0
    while [ "$waited" -lt 40 ]; do
        "$AXTEXT" "$APP_NAME" 2>/dev/null >"$WORK/ax.txt"
        grep -qF "$words" "$WORK/ax.txt" && return 0
        sleep 0.25
        waited=$((waited + 1))
    done
    return 1
}
question_gone() {  # wait for the windows read through without the question in them
    local waited=0
    while [ "$waited" -lt 40 ]; do
        "$AXTEXT" "$APP_NAME" 2>/dev/null >"$WORK/ax.txt" \
            && grep -q '^=== window' "$WORK/ax.txt" \
            && ! grep -qF "para a lixeira" "$WORK/ax.txt" && return 0
        sleep 0.25
        waited=$((waited + 1))
    done
    return 1
}
# The foot of the screen's usable part, in the points axpress counts from the top.
SCREEN_FOOT=$(osascript -l JavaScript -e 'ObjC.import("AppKit")
var s = $.NSScreen.screens.objectAtIndex(0)
s.frame.size.height - s.visibleFrame.origin.y' 2>/dev/null)
on_screen() {  # on_screen <button>: yes when all of it is above the foot of the screen
    python3 -c 'import sys
f = sys.argv[1].split()
if len(f) != 4: print("no frame"); sys.exit()
x, y, w, h = map(float, f)
print("yes" if y >= 0 and y + h <= float(sys.argv[2]) else "no: %s, the foot at %s" % (sys.argv[1], sys.argv[2]))' \
        "$("$AXPRESS" "$APP_NAME" frame "$1" 2>/dev/null)" "${SCREEN_FOOT:-0}"
}
tabs() { "$APP/Contents/Resources/keep" ls 2>/dev/null | grep -cE '^ +tab [0-9]+'; }

say ""
say "the question, and Cancelar"
"$AXPRESS" "$APP_NAME" menu File "Close Tab"
check "File > Close Tab puts up a question naming the worktree" yes "$(question_up && echo yes || echo no)"
check "it says where the worktree is" yes "$(grep -qF "$WT_NAME (ramo ramo-do-teste; 1 arquivo alterado)" "$WORK/ax.txt" && echo yes || echo no)"
check "and how to get it back" yes "$(grep -qF "Colocar de volta" "$WORK/ax.txt" && echo yes || echo no)"
check "the helper was asked which worktrees, and nothing else" "listar" "$(asked)"
"$AXPRESS" "$APP_NAME" button Cancelar
sleep 3
check "Cancelar: the worktree stays" yes "$([ -f "$WT/arquivo.txt" ] && echo yes || echo no)"
check "Cancelar: nothing prepared or concluded" "listar" "$(asked)"
check "Cancelar: nothing in the Trash" no "$([ -e "$HOME/.Trash/$WT_NAME" ] && echo yes || echo no)"

say ""
say "forty worktrees: the question fits, and escape"
cp "$WORK/listar.json" "$WORK/listar-um.json"
python3 - "$WORK/listar.json" "$WORK" <<'PY'
import json, sys
json.dump({"versao": 1, "abas": [{"alvo": "w:1", "pids": []}],
           "lixeira": [{"caminho": "%s/de-uma-conversa-longa/wt-%02d" % (sys.argv[2], i),
                        "ramo": "fix/um-ramo-de-nome-comprido-%02d" % i, "head": "abc1234",
                        "alteracoes": 0, "commits_so_aqui": 0, "processos": []} for i in range(1, 41)],
           "mantidas": [], "avisos": []}, open(sys.argv[1], "w"))
PY
rm -f "$WORK/chamadas.log"
"$AXPRESS" "$APP_NAME" menu File "Close Tab"
check "File > Close Tab puts up the question with the forty" yes \
    "$(question_up "As 40 worktrees desta aba vão para a lixeira" && echo yes || echo no)"
check "the fortieth is in it" yes \
    "$(grep -qF "wt-40 (ramo fix/um-ramo-de-nome-comprido-40)" "$WORK/ax.txt" && echo yes || echo no)"
check "Fechar Aba is on the screen" yes "$(on_screen "Fechar Aba")"
check "Cancelar is on the screen" yes "$(on_screen Cancelar)"
"$SENDKEY" "$APP_PID" key 53 >/dev/null 2>&1
escaped=$(question_gone && echo yes || echo no)
check "escape puts it away" yes "$escaped"
# Put away by its button when escape did not, so what follows starts clean.
[ "$escaped" = yes ] || { "$AXPRESS" "$APP_NAME" button Cancelar >/dev/null 2>&1; question_gone; }
check "escape: nothing prepared or concluded" "listar" "$(asked)"
check "escape: the tab stays" 1 "$(tabs)"
cp "$WORK/listar-um.json" "$WORK/listar.json"

say ""
say "the question, and Fechar Aba"
rm -f "$WORK/chamadas.log"
check "before: the daemon has the tab" 1 "$(tabs)"
"$AXPRESS" "$APP_NAME" menu File "Close Tab"
check "asked again, the question is up again" yes "$(question_up && echo yes || echo no)"
# A return typed into the terminal as the question comes up ends nothing.
# Read off the sheet rather than typed: a return posted to an app behind
# presses no default button, whichever there is (measured).
check "return presses no button of it" "" "$("$AXPRESS" "$APP_NAME" default)"
"$AXPRESS" "$APP_NAME" button "Fechar Aba"
waited=0
while [ -e "$WT" ] && [ "$waited" -lt 60 ]; do sleep 0.25; waited=$((waited + 1)); done
check "Fechar Aba: the worktree left its place" no "$([ -e "$WT" ] && echo yes || echo no)"
check "and is in the Trash with what was in it" yes "$([ -f "$HOME/.Trash/$WT_NAME/arquivo.txt" ] && echo yes || echo no)"
sleep 1
check "the helper listed, prepared, then concluded" "listar preparar concluir" "$(asked)"
concluded=$(python3 -c 'import json,sys
for l in open(sys.argv[1]):
    a = json.loads(l)["argv"]
    if a[0] == "concluir": print(a[2])' "$WORK/chamadas.log")
check "conclude was told where the Trash put it" "$HOME/.Trash/$WT_NAME" "$concluded"
check "after: the tab is gone from the daemon" 0 "$(tabs)"

say ""
say "$PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
