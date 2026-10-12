#!/usr/bin/env bash
#
# The bar under a Claude Code conversation: the model and the effort it runs
# with, how many agents it has going, and a click on either of the first two
# to change it.
#
# The tab's program poses as Claude Code: it draws the prompt box and the list
# of agents the way Claude Code does, leaves the status line's file for its
# own shell (KEEP_STATUS_DIR), and writes down what it is sent. The bar is read
# and clicked through the accessibility tree (tools/axpress.swift).
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


# The tab's program: poses as Claude Code.
cat >"$WORK/printer" <<'PY'
#!/usr/bin/env python3
import json, os, sys, tty
WORK = os.environ["KEEP_BAR_WORK"]
tty.setraw(0)
out = sys.stdout
effort = os.environ.get("BAR_EFFORT", "xhigh")
model = ("claude-opus-5-5[1m]", "Opus 5.5 (1M context)")
def status():
    # For the tab's shell, which is the program above this one (see below).
    import subprocess
    pids = {os.getppid()}
    pids.add(int(subprocess.check_output(["ps", "-o", "ppid=", "-p", str(os.getppid())]).split()[0]))
    for pid in pids:
        with open(os.path.join(os.environ["KEEP_STATUS_DIR"], "%d.json" % pid), "w") as f:
            json.dump({"model": {"id": model[0], "display_name": model[1]}, "effort": {"level": effort}}, f)
def draw():
    rule = "─" * 100
    out.write("\x1b]2;✳ teste\x07\x1b[2J\x1b[H")
    out.write("\r\n" * 3 + rule + "\r\n❯ \r\n" + rule + "\r\n")
    out.write("  ⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents\r\n\r\n")
    out.write("  ⏺ main\r\n  ◯ general-purpose  Reading files\r\n  ◯ Explore  Scanning\r\n  ◯ Plan  Thinking\r\n")
    out.write("\x1b[5;3H")
    out.flush()
status()
draw()
open(os.path.join(WORK, "ready"), "w").close()
raw = b""
while True:
    data = os.read(0, 256)
    if not data:
        break
    raw += data
    with open(os.path.join(WORK, "bytes"), "w") as f:
        f.write(raw.decode("utf-8", "replace").replace("\r", "<CR>"))
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
say "the bar under the conversation"
bar() {  # bar <identifier> <what> -> the element's title or value
    "$AXPRESS" "$APP_NAME" "$2" "$1" 2>/dev/null
}
for _ in $(seq 20); do
    [ -n "$(bar claude-model title)" ] && break
    sleep 1
done
check "the model" "Opus 5.5 (1M) ▾" "$(bar claude-model title)"
check "the effort" "esforço xhigh ▾" "$(bar claude-effort title)"
check "the agents" "3 agentes" "$(bar claude-agents value)"

say ""
say "clicking the effort"
"$AXPRESS" "$APP_NAME" open claude-effort >/dev/null 2>&1
sleep 1
ITEMS=$("$AXPRESS" "$APP_NAME" items 2>/dev/null | cut -f3 | tr '\n' '|')
check "it offers the five levels" "low|medium|high|xhigh|max|" "$ITEMS"
"$AXPRESS" "$APP_NAME" pick max >/dev/null 2>&1
sleep 2
check "the conversation is sent /effort max" "/effort max<CR>" "$(cat "$WORK/bytes" 2>/dev/null)"

say ""
say "clicking the model"
ITEMS=
for _ in 1 2 3; do
    "$AXPRESS" "$APP_NAME" open claude-model >/dev/null 2>&1
    sleep 1
    ITEMS=$("$AXPRESS" "$APP_NAME" items 2>/dev/null | cut -f3 | tr '\n' '|')
    [ -n "$ITEMS" ] && break
done
check "it offers the models" "Fable 5.1|Opus 5.5|Opus 5.5 (1M)|Sonnet 5.5|Haiku 5.5|" "$ITEMS"
"$AXPRESS" "$APP_NAME" pick "Sonnet 5.5" >/dev/null 2>&1
sleep 2
check "the conversation is then sent /model claude-sonnet-5-5" "/effort max<CR>/model claude-sonnet-5-5<CR>" "$(cat "$WORK/bytes" 2>/dev/null)"

stop_app
say ""
say "passed $PASSED, failed $FAILED"
[ "$FAILED" -eq 0 ]
