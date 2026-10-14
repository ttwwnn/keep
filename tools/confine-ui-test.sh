#!/usr/bin/env bash
#
# The lock between a workspace's name and its +: shut when the AI is held to a
# folder, with a menu that says which and lets the AI go again.
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
swiftc -O tools/mousedrag.swift -o "$MOUSE" 2>/dev/null || { say "could not build mousedrag"; exit 1; }
swiftc -O tools/axpress.swift -o "$AXPRESS" 2>/dev/null || { say "could not build axpress"; exit 1; }

export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$WORK/state
export KEEP_CONFINE_CONFIG=$WORK/confinement.json
printf '{"link": "%s"}\n' "$WORK" > "$KEEP_CONFINE_CONFIG"
mkdir -p "$KEEP_STATE_DIR"
require_scratch_socket
rm -f "$SOCKET"

TARGET=$WORK/nota.txt
: >"$TARGET"
LONG=$WORK/projetos/clinica/app/Filament/Resources/relatorio_de_atendimento_completo_do_paciente.txt
mkdir -p "$(dirname "$LONG")"
: >"$LONG"


# The tab's program: only there to be a tab.
cat >"$WORK/printer" <<'PY'
#!/usr/bin/env python3
import os, time
open(os.path.join(os.environ["KEEP_BAR_WORK"], "ready"), "w").close()
time.sleep(600)
PY
chmod +x "$WORK/printer"
export KEEP_BAR_WORK=$WORK

SHELL="$WORK/printer" ./target/release/keepd >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
sleep 1
./target/release/keep new link >/dev/null 2>&1 || { say "could not make a workspace"; exit 1; }
cat >"$KEEP_STATE_DIR/windows.json" <<'JSON'
{"0":{"x":40,"y":160,"width":1100,"height":700,"workspaces":["link"],"tab":{"workspace":"link","root":1}}}
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
say "a workspace with a folder"
lock() { "$AXPRESS" "$APP_NAME" frame confine-link 2>/dev/null; }
for _ in $(seq 20); do [ -n "$(lock)" ] && break; sleep 1; done
check "the lock is between the name and the +" "yes" "$([ -n "$(lock)" ] && echo yes || echo no)"

say ""
say "the lock's menu"
ITEMS=
for _ in 1 2 3; do
    "$AXPRESS" "$APP_NAME" open confine-link >/dev/null 2>&1
    sleep 1
    ITEMS=$("$AXPRESS" "$APP_NAME" items 2>/dev/null | cut -f3 | grep -E 'Trocar|Liberar|só acessa' | tr '\n' '|')
    [ -n "$ITEMS" ] && break
done
check "it says the folder, and offers to change it and to let the AI go" \
    "A IA só acessa $WORK|Trocar a pasta…|Liberar a IA|" "$ITEMS"
"$AXPRESS" "$APP_NAME" pick "Liberar a IA" >/dev/null 2>&1
sleep 2
check "the folder is taken back from the file" "{}" \
    "$(python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))))' "$KEEP_CONFINE_CONFIG")"

stop_app
say ""
say "passed $PASSED, failed $FAILED"
[ "$FAILED" -eq 0 ]
