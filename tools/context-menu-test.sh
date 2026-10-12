#!/usr/bin/env bash
#
# Right-click on a file path printed in a tab puts up a menu with Abrir, Mostrar
# no Finder and Copiar caminho; on text that names no file there is no menu.
#
# The tab's program asks for the mouse, as Claude Code's full-screen mode
# does. The right button is pressed by the window server
# (tools/mousedrag.swift) and the menu read and chosen through the
# accessibility tree (tools/axpress.swift). KEEP_LINK_LOG makes the build
# write down what it would have opened or revealed instead of doing it.
#
#   tools/context-menu-test.sh
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
SOCKET=/tmp/keep-ctx-$$.sock
WORK=$(mktemp -d /tmp/keep-ctx-XXXXXX)
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
export KEEP_LINK_LOG=$WORK/opened
mkdir -p "$KEEP_STATE_DIR"
require_scratch_socket
rm -f "$SOCKET"
: >"$KEEP_LINK_LOG"

TARGET=$WORK/nota.txt
: >"$TARGET"
LONG=$WORK/projetos/clinica/app/Filament/Resources/relatorio_de_atendimento_completo_do_paciente.txt
mkdir -p "$(dirname "$LONG")"
: >"$LONG"

# The tab's program: asks for the mouse, fills the screen with the path and
# some text that names nothing, and waits.
cat >"$WORK/printer" <<PY
#!/usr/bin/env python3
import sys, time
sys.stdout.write("\x1b[?1000h\x1b[?1002h\x1b[?1006h")
for _ in range(12):
    print(("$TARGET:12  " * 2).rstrip())
for _ in range(12):
    print("palavra sem arquivo nenhum aqui " * 5)
open("$WORK/ready", "w").close()
time.sleep(300)
PY
chmod +x "$WORK/printer"

SHELL="$WORK/printer" ./target/release/keepd >"$WORK/daemon.log" 2>&1 &
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

read -r FX FY FW FH <<<"$("$MOUSE" frame)"
[ -n "${FH:-}" ] || { say "could not find $APP_NAME's window"; exit 1; }
# The terminal's upper part, clear of the sidebar and of the tab strip.
# On the second copy of the path in a row, clear of the sidebar.
TX=$(( ${FX%.*} + 560 ))
TY=$(( ${FY%.*} + 120 ))


# A click on text that names no file, so the pane has the keyboard and the
# right-click that follows is the first thing it hears from the pointer.
"$MOUSE" click "$TX" "$((TY + 12 * 17))"
sleep 1
: >"$KEEP_LINK_LOG"
SAVED_CLIP=$(pbpaste)
say ""
say "right-click on the path"
"$MOUSE" rightclick "$TX" "$TY"
sleep 1
# The system adds its own entries (AutoFill) after ours.
ITEMS=$("$AXPRESS" "$APP_NAME" items 2>/dev/null | cut -f3 | grep -E '^(Abrir|Mostrar no Finder|Copiar caminho)$' | tr '\n' '|')
check "the menu offers the three actions" "Abrir|Mostrar no Finder|Copiar caminho|" "$ITEMS"
"$AXPRESS" "$APP_NAME" pick "Mostrar no Finder" >/dev/null 2>&1
sleep 1
check "Reveal in Finder shows the file" "reveal $TARGET" "$(cat "$KEEP_LINK_LOG")"

: >"$KEEP_LINK_LOG"
"$MOUSE" rightclick "$TX" "$TY"
sleep 1
"$AXPRESS" "$APP_NAME" pick "Abrir" >/dev/null 2>&1
sleep 1
check "Open opens the file" "$TARGET" "$(cat "$KEEP_LINK_LOG")"

"$MOUSE" rightclick "$TX" "$TY"
sleep 1
"$AXPRESS" "$APP_NAME" pick "Copiar caminho" >/dev/null 2>&1
sleep 1
COPIED=$(pbpaste)
printf %s "$SAVED_CLIP" | pbcopy
check "Copy Path puts the path on the pasteboard" "$TARGET" "$COPIED"

say ""
say "right-click on text that names no file"
"$MOUSE" rightclick "$TX" "$((TY + 12 * 17))"
sleep 1
ITEMS=$("$AXPRESS" "$APP_NAME" items 2>/dev/null | cut -f3 | grep -E '^(Abrir|Mostrar no Finder|Copiar caminho)$' | tr '\n' '|')
"$AXPRESS" "$APP_NAME" cancel >/dev/null 2>&1
check "there is no menu" "" "$ITEMS"

stop_app
say ""
say "passed $PASSED, failed $FAILED"
[ "$FAILED" -eq 0 ]
