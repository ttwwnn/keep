#!/usr/bin/env bash
#
# ⌘V pastes the clipboard into the tab, bracketed when the program asked (as
# Claude Code does).
#
#
# The tab's program is a tiny line editor (it understands Right, Left and
# Backspace and writes down what is in its line). It is clicked for real, the
# keys are posted to the app's pid, and what the
# program holds afterwards is read from a file.
#
#   tools/paste-test.sh
#
# Your app and your daemon are never touched. It takes the pointer for a few
# seconds, and the clipboard is put back.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=$(dev_app)
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
SOCKET=/tmp/keep-cut-$$.sock
WORK=$(mktemp -d /tmp/keep-cut-XXXXXX)
MOUSE=$WORK/mousedrag
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


# The tab's program: a line of text with the cursor at its end.
cat >"$WORK/printer" <<'PY'
#!/usr/bin/env python3
import os, select, sys, termios, tty, time
WORK = os.environ["KEEP_CUT_WORK"]
line = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
pos = int(os.environ.get("CUT_POS", len(line)))
tty.setraw(0)
import base64
# As Claude Code in full screen: it takes the mouse, selects by itself and
# copies what it selected when the button is let go (OSC 52).
sys.stdout.write("\x1b[?2004h")
out = sys.stdout
def save(name, text):
    with open(os.path.join(WORK, name), "w") as f:
        f.write(text)
def draw():
    out.write("\r\x1b[K❯ " + line + ("\x1b[%dG" % (3 + pos)) + os.environ.get("MARK",""))
    out.flush()
    save("line", line)
out.write("\n" * 4)
draw()
open(os.path.join(WORK, "ready"), "w").close()
raw = b""
redrawn = False
while True:
    if not select.select([0], [], [], 0.2)[0]:
        if not redrawn and os.path.exists(os.path.join(WORK, "redraw")):
            redrawn = True
            draw()
        continue
    data = os.read(0, 64)
    if not data:
        break
    raw += data
    save("bytes", raw.hex())
    if b"\x1b[<" in data and data.rstrip().endswith(b"m"):
        out.write("\x1b]52;c;" + base64.b64encode(line.encode()).decode() + "\x07")
        out.flush()
        continue
    i = 0
    while i < len(data):
        if data[i:i+3] == b"\x1b[C":
            pos = min(len(line), pos + 1); i += 3
        elif data[i:i+3] == b"\x1b[D":
            pos = max(0, pos - 1); i += 3
        elif data[i] == 0x7f:
            if pos > 0:
                line = line[:pos-1] + line[pos:]; pos -= 1
            i += 1
        else:
            i += 1
    draw()
PY
chmod +x "$WORK/printer"
export KEEP_CUT_WORK=$WORK

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

touch "$WORK/redraw"   # the program draws its line again, now the window is settled
sleep 1
SENDKEY=$WORK/sendkey
swiftc -O tools/sendkey.swift -o "$SENDKEY" 2>/dev/null || { say "could not build sendkey"; exit 1; }

APP_PID_REAL=$APP_PID
SAVED_CLIP=$(pbpaste)
printf %s "before" | pbcopy
WORD=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA

say ""
say "⌘V in a tab whose program only reads a line"
printf %s "abc#DEF123" | pbcopy
rm -f "$WORK/bytes"
osascript -e "tell application \"System Events\" to set frontmost of process \"$APP_NAME\" to true" >/dev/null 2>&1
sleep 1
"$SENDKEY" "$APP_PID_REAL" key 9 cmd
sleep 2
GOT=$(xxd -r -p "$WORK/bytes" 2>/dev/null | tr -d '\033')
check "the text arrives (bracketed when the program asked)" "[200~abc#DEF123[201~" "$GOT"
printf %s "$SAVED_CLIP" | pbcopy

stop_app
say ""
say "passed $PASSED, failed $FAILED"
[ "$FAILED" -eq 0 ]
