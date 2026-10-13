#!/usr/bin/env bash
#
# Typing `claude` in a folder Claude Code does not know: it asks whether to
# trust the folder, and Keep answers yes.
#
# The tab's program poses as Claude Code: it draws the question and writes
# down what it is sent.
#
#   tools/status-bar-test.sh
#
# Your app and your daemon are never touched.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=$(dev_app)
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
SOCKET=/tmp/keep-bar-$$.sock
WORK=$(mktemp -d /tmp/keep-bar-XXXXXX)
MOUSE=$WORK/mousedrag
AXPRESS=$WORK/axpress
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

say "building $APP_NAME, the client, the daemon and the tools (yours is left alone)"
./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }
swiftc -O tools/axpress.swift -o "$AXPRESS" 2>/dev/null || { say "could not build axpress"; exit 1; }

export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$WORK/state
export KEEP_STATUS_DIR=$WORK/status
mkdir -p "$KEEP_STATUS_DIR"
mkdir -p "$KEEP_STATE_DIR"
require_scratch_socket
rm -f "$SOCKET"

TARGET=$WORK/nota.txt
: >"$TARGET"
LONG=$WORK/projetos/clinica/app/Filament/Resources/relatorio_de_atendimento_completo_do_paciente.txt
mkdir -p "$(dirname "$LONG")"
: >"$LONG"


# The tab's program: poses as Claude Code opening in a folder it does not know.
cat >"$WORK/printer" <<'PY'
#!/usr/bin/env python3
import os, sys, tty
WORK = os.environ["KEEP_BAR_WORK"]
tty.setraw(0)
out = sys.stdout
pointer = 0   # 0: on "No, exit" (where Claude Code starts), 1: on "Yes, I trust this folder"
def trust():
    out.write("\x1b]2;✳ teste\x07\x1b[2J\x1b[H")
    out.write("\r\n Accessing workspace:\r\n\r\n /Users/thiago/projetos/novo\r\n\r\n")
    out.write(" Quick safety check: Is this a project you created or one you trust? (Like your own code, a well-known open source project, or work from your\r\n team). If not, take a moment to review what's in this folder first.\r\n\r\n")
    out.write((" ❯ " if pointer == 0 else "   ") + "No, exit\r\n" + (" ❯ " if pointer == 1 else "   ") + "Yes, I trust this folder\r\n\r\n Enter to confirm · Esc to cancel\r\n")
    out.flush()
def prompt():
    rule = "─" * 100
    out.write("\x1b[2J\x1b[H\r\n\r\n" + rule + "\r\n❯ \r\n" + rule + "\r\n  ⏵⏵ bypass permissions on\r\n")
    out.flush()
trust()
open(os.path.join(WORK, "ready"), "w").close()
while True:
    data = os.read(0, 256)
    if not data:
        break
    with open(os.path.join(WORK, "bytes"), "a") as f:
        f.write(data.decode("utf-8", "replace").replace("\r", "<CR>").replace("\x1b", "<ESC>"))
    if data == b"\x1b[B":
        pointer = 1
        trust()
    elif data == b"\r":
        if pointer == 1:
            prompt()
        else:
            out.write("\x1b[2J\x1b[H EXIT\r\n"); out.flush()
PY
chmod +x "$WORK/printer"
export KEEP_BAR_WORK=$WORK

# Claude Code's process is called after its version, which is how Keep knows
# what runs in the tab: a program of that name holds the terminal and runs the
# printer, and is the shell the status file is named after.
cat >"$WORK/fake.c" <<C
#include <stdlib.h>
int main(void) { return system("$WORK/printer"); }
C
cc -o "$WORK/2.1.296" "$WORK/fake.c" || { say "could not build the stand-in"; exit 1; }
SHELL="$WORK/2.1.296" ./target/release/keepd >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
sleep 1
./target/release/keep new link >/dev/null 2>&1 || { say "could not make a workspace"; exit 1; }
cat >"$KEEP_STATE_DIR/windows.json" <<'JSON'
{"0":{"x":360,"y":160,"width":1100,"height":700,"workspaces":["link"],"tab":{"workspace":"link","root":1}}}
JSON

stop_app
KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
APP_PID=$!
waited=0
while [ ! -e "$WORK/ready" ] && [ "$waited" -lt 60 ]; do sleep 0.25; waited=$((waited + 1)); done
sleep 3
place_on_screen
osascript -e "tell application \"System Events\" to set frontmost of process \"$APP_NAME\" to true" \
    >/dev/null 2>&1
sleep 1

# The window may still be taking its final size (the grid reflows, and rows
# move): wait until the app has said nothing about layout for two seconds.
for _ in $(seq 40); do
    before=$(grep -c ' layout ' "$WORK/app.log")
    sleep 2
    [ "$(grep -c ' layout ' "$WORK/app.log")" = "$before" ] && break
done


say ""
say "the question on the screen"
for _ in $(seq 20); do
    [ -s "$WORK/bytes" ] && break
    sleep 1
done
sleep 12
check "the pointer goes down to the yes, then Enter, once each" "<ESC>[B<CR>" "$(cat "$WORK/bytes" 2>/dev/null)"

stop_app
say ""
say "passed $PASSED, failed $FAILED"
[ "$FAILED" -eq 0 ]
