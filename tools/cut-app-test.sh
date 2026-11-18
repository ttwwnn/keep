#!/usr/bin/env bash
#
# ⌘X on text selected in the line a program is editing cuts it: the text goes
# to the clipboard and the program is told, with keys, to take it out. ⌘⇧V
# sends the program ^V, which is how Claude Code is asked to read an image on
# the clipboard. Shift with the left arrow marks text in the prompt box, and
# typing over the mark replaces it. A program that selects with the mouse
# itself, as Claude Code in full screen does (it copies on release and takes
# its selection out on one backspace), gets that one backspace and no more.
#
# The tab's program is a tiny line editor (it understands Right, Left and
# Backspace and writes down what is in its line). The word is selected with a
# real double click, the keys are posted to the app's pid, and what the
# program holds afterwards is read from a file.
#
#   tools/cut-app-test.sh
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
LONG=$WORK/projetos/estoque/app/Filament/Resources/relatorio_de_atendimento_completo_do_paciente.txt
mkdir -p "$(dirname "$LONG")"
: >"$LONG"


# The tab's program: a line of text with the cursor at its end.
cat >"$WORK/printer" <<'PY'
#!/usr/bin/env python3
import os, select, subprocess, sys, termios, tty, time
WORK = os.environ["KEEP_CUT_WORK"]
line = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
pos = int(os.environ.get("CUT_POS", len(line)))
# Once the file "mouse" is there, the program has the mouse, as Claude Code in
# full screen: a press selects the word under it, the release copies it, and a
# backspace takes the selection out whole. An arrow or a letter lets it go
# (a letter does not type over it). "reset" puts the line back.
sel = None
def reset():
    global line, pos, sel
    line, sel = "um dois " + "A" * 70, None
    pos = len(line)
tty.setraw(0)
out = sys.stdout
def save(name, text):
    with open(os.path.join(WORK, name), "w") as f:
        f.write(text)
def draw():
    # On the row the test aims at, once it has said which (see "row" below).
    # Between two rules, as Claude Code draws its prompt box.
    try:
        row = int(open(os.path.join(WORK, "row")).read()) + 1
        out.write("\x1b[%d;1H\x1b[K%s\x1b[%d;1H\x1b[K%s\x1b[%d;1H" % (row - 1, "─" * 40, row + 1, "─" * 40, row))
    except (OSError, ValueError):
        pass
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
        if os.path.exists(os.path.join(WORK, "reset")):
            os.remove(os.path.join(WORK, "reset"))
            if os.path.exists(os.path.join(WORK, "mouse")):
                out.write("\x1b[?1000h\x1b[?1006h")
            reset()
            draw()
        continue
    data = os.read(0, 64)
    if not data:
        break
    raw += data
    save("bytes", raw.hex())
    i = 0
    while i < len(data):
        if data[i:i+3] == b"\x1b[<":
            end = next((k for k in range(i + 3, len(data)) if data[k] in b"Mm"), len(data) - 1)
            button, x, _ = (int(n) for n in data[i+3:end].split(b";"))
            column = x - 3
            if data[end] == ord("M") and button == 0 and 0 <= column < len(line) and line[column] != " ":
                start = line.rfind(" ", 0, column) + 1
                stop = line.find(" ", column)
                sel = (start, len(line) if stop < 0 else stop)
            elif data[end] == ord("m") and sel:
                subprocess.run(["pbcopy"], input=line[sel[0]:sel[1]].encode())
            i = end + 1
        elif data[i:i+3] == b"\x1b[C":
            sel = None; pos = min(len(line), pos + 1); i += 3
        elif data[i:i+3] == b"\x1b[D":
            sel = None; pos = max(0, pos - 1); i += 3
        elif data[i] == 0x7f and sel:
            line = line[:sel[0]] + line[sel[1]:]; pos = sel[0]; sel = None; i += 1
        elif data[i] == 0x7f:
            if pos > 0:
                line = line[:pos-1] + line[pos:]; pos -= 1
            i += 1
        elif 0x20 <= data[i] < 0x7f:
            sel = None
            line = line[:pos] + chr(data[i]) + line[pos:]; pos += 1; i += 1
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
# Wide enough for the program's line (82 cells) beside the sidebar, which no
# longer narrows below about 330 points: a line that wraps runs into the rule.
cat >"$KEEP_STATE_DIR/windows.json" <<'JSON'
{"0":{"x":40,"y":160,"width":1300,"height":700,"workspaces":["link"],"tab":{"workspace":"link","root":1}}}
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

# The row under the aim, as the app counts it: a plain click there on blank
# says "plain click on nothing: row N". The program draws its line on that
# row, so the aim holds whatever the text's size (it is 25% larger since
# b9e3d21, and a fixed 120 points below the window's top fell on the row
# above the line).
BEFORE_AIM=$(grep -c 'plain click on nothing' "$WORK/app.log")
"$MOUSE" click "$TX" "$TY"
# Under load the line can take seconds to be written: wait for it, not a fixed time.
for _ in $(seq 40); do
    [ "$(grep -ac 'plain click on nothing' "$WORK/app.log")" -gt "$BEFORE_AIM" ] && break
    sleep 0.25
done
AIM_ROW=$(grep -a 'plain click on nothing: row' "$WORK/app.log" | tail -n +"$((BEFORE_AIM + 1))" \
    | sed -nE 's/.*row ([0-9]+),.*/\1/p' | tail -1)
[ -n "$AIM_ROW" ] || { say "the click on the line was not heard: no row to aim at"; exit 1; }
[ -n "$AIM_ROW" ] && printf %s "$AIM_ROW" >"$WORK/row"
touch "$WORK/redraw"   # the program draws its line again, now the window is settled
sleep 1
SENDKEY=$WORK/sendkey
swiftc -O tools/sendkey.swift -o "$SENDKEY" 2>/dev/null || { say "could not build sendkey"; exit 1; }

APP_PID_REAL=$APP_PID
SAVED_CLIP=$(pbpaste)
printf %s "before" | pbcopy
WORD=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA

say ""
say "a word selected in the line, then ⌘X"
activate() {
    osascript -e "tell application \"System Events\" to set frontmost of process \"$APP_NAME\" to true" \
        >/dev/null 2>&1
    sleep 1
}
activate
"$MOUSE" click "$TX" "$((TY + 12 * 17))"
sleep 1
activate
"$MOUSE" dblclick "$TX" "$TY"
sleep 1
"$SENDKEY" "$APP_PID_REAL" key 7 cmd
sleep 2
check "the program's line is empty" "" "$(cat "$WORK/line")"
check "the clipboard has the word" "$WORD" "$(pbpaste)"

say ""
say "⌘X with nothing selected"
rm -f "$WORK/bytes"
"$SENDKEY" "$APP_PID_REAL" key 7 cmd
sleep 1
check "the program is sent nothing" "" "$(cat "$WORK/bytes" 2>/dev/null)"

say ""
say "⌘⇧V"
rm -f "$WORK/bytes"
"$SENDKEY" "$APP_PID_REAL" key 9 cmd shift
sleep 1
# The program keeps everything it was ever sent; the file is back only if it was sent more.
check "the program gets ^V last" "16" "$(tail -c 2 "$WORK/bytes" 2>/dev/null)"

say ""
say "Shift with the left arrow marks text in the prompt box"
"$SENDKEY" "$APP_PID_REAL" text "abcdef"
sleep 1
check "the word is typed" "abcdef" "$(cat "$WORK/line")"
for _ in 1 2 3; do "$SENDKEY" "$APP_PID_REAL" key 123 shift; sleep 0.4; done
sleep 0.5
"$SENDKEY" "$APP_PID_REAL" key 8 cmd
sleep 1
check "⌘C copies the three letters marked" "def" "$(pbpaste)"
"$SENDKEY" "$APP_PID_REAL" text "X"
sleep 1.5
check "typing over the mark replaces it" "abcX" "$(cat "$WORK/line")"

say ""
say "a program that selects with the mouse itself (Claude Code in full screen)"
A70=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
touch "$WORK/mouse" "$WORK/reset"
sleep 1.5
check "the line is back, the word last" "um dois $A70" "$(cat "$WORK/line")"
select_last_word() {  # the program selects the word under the press and copies it on release
    touch "$WORK/reset"
    sleep 1
    printf %s "before" | pbcopy
    activate
    "$MOUSE" click "$TX" "$TY"
    sleep 1
}
select_last_word
check "the program copied the word" "$A70" "$(pbpaste)"
"$SENDKEY" "$APP_PID_REAL" key 51
sleep 1.5
check "backspace takes out the selection and nothing before it" "um dois " "$(cat "$WORK/line")"
select_last_word
"$SENDKEY" "$APP_PID_REAL" text "X"
sleep 1.5
check "typing over the program's selection replaces it" "um dois X" "$(cat "$WORK/line")"
select_last_word
"$SENDKEY" "$APP_PID_REAL" key 7 cmd
sleep 1.5
check "⌘X takes it out" "um dois " "$(cat "$WORK/line")"
check "⌘X leaves the word on the clipboard" "$A70" "$(pbpaste)"
select_last_word
"$SENDKEY" "$APP_PID_REAL" key 123
sleep 0.5
"$SENDKEY" "$APP_PID_REAL" text "Y"
sleep 1.5
check "after an arrow lets it go, typing erases nothing" "um dois ${A70%A}YA" "$(cat "$WORK/line")"

printf %s "$SAVED_CLIP" | pbcopy
stop_app
say ""
say "passed $PASSED, failed $FAILED"
[ "$FAILED" -eq 0 ]
