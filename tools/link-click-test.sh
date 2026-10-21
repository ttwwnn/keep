#!/usr/bin/env bash
#
# ⌘-click on a file path printed in a tab opens that file.
#
# The tab prints a path (with the :line a compiler would append) over and over
# until the screen is full of it, so the click lands on it wherever it falls.
# The pointer is moved by the window server (tools/mousedrag.swift), so what is
# under test includes the runtime finding the path under the pointer and
# handing it to the app, not only what the app does with it. KEEP_LINK_LOG
# makes the build write down what it would have opened instead of opening it.
#
#   tools/link-click-test.sh
#
# Checked:
#   - a plain click opens the file too; a double click, a drag across it and
#     the click that brings the window forward open nothing;
#   - a click with ⌘ opens the file, the :line left off;
#   - the same when the pointer is resting on the path and ⌘ is pressed after;
#   - all of it again with the tab's program having asked for the mouse;
#   - a path the program broke across two rows, as Claude Code does with one
#     wider than the screen, opens whole from either row;
#   - web addresses (PRINT=web, osc8, webwrapped): a plain click on one
#     opens it in the browser, a double click does not; a markdown link whose
#     address is behind its text (OSC 8) opens that address; one broken
#     across rows opens whole, plain click or ⌘-click; the right button
#     offers "Abrir link". With the mouse taken, as Claude Code in full
#     screen takes it, the plain click opens nothing here: the program has
#     it and opens its own links (opening it here too opened it twice);
#     ⌘-click and the menu still open it.
#   - a pasted picture (PRINT=image): the `[Image #1]` Claude Code draws, with
#     the file behind it (OSC 8). A plain click opens the picture, also with
#     the mouse taken, and then the program never sees the press (Claude Code
#     would only show the file in the Finder); a double click opens nothing;
#     a drag across it reaches the program, so selecting still works;
#   - a picture still in the box being typed (PRINT=pasted): Claude Code has
#     not written it yet and puts no link behind its `[Image #N]`, so the app
#     keeps a copy of the clipboard at ⌘⇧V and a plain click opens that.
#
# It takes the pointer for a few seconds. Your app and your daemon are never
# touched.

set -uo pipefail
cd "$(dirname "$0")/.."
# Twice: the tab's program leaving the mouse alone, and asking for it as Claude
# Code's full-screen mode does (MOUSE_MODE=1), where ⌘-click used to reach the
# program and never the link.
# Then once more with the path broken across rows (PRINT=wrapped).
if [ -z "${MOUSE_MODE:-}" ]; then
    status=0
    for mode in 0 1; do
        echo "== the program takes the mouse: $([ "$mode" = 1 ] && echo yes || echo no)"
        MOUSE_MODE=$mode "$0" || status=1
    done
    echo "== a path broken across two rows, the mouse taken"
    MOUSE_MODE=1 PRINT=wrapped "$0" || status=1
    for print in web osc8 webwrapped; do
        for mode in 0 1; do
            echo "== web addresses ($print), the program takes the mouse: $([ "$mode" = 1 ] && echo yes || echo no)"
            MOUSE_MODE=$mode PRINT=$print "$0" || status=1
        done
    done
    for mode in 0 1; do
        echo "== a pasted picture, the program takes the mouse: $([ "$mode" = 1 ] && echo yes || echo no)"
        MOUSE_MODE=$mode PRINT=image "$0" || status=1
        echo "== a picture in the box being typed, the program takes the mouse: $([ "$mode" = 1 ] && echo yes || echo no)"
        MOUSE_MODE=$mode PRINT=pasted "$0" || status=1
    done
    exit $status
fi
PRINT=${PRINT:-plain}
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=$(dev_app)
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
SOCKET=/tmp/keep-link-$$.sock
WORK=$(mktemp -d /tmp/keep-link-XXXXXX)
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

IMAGE=$WORK/images/1.png
mkdir -p "$(dirname "$IMAGE")"
printf '\x89PNG\r\n\x1a\n' >"$IMAGE"
TARGET=$WORK/nota.txt
: >"$TARGET"
LONG=$WORK/projetos/clinica/app/Filament/Resources/relatorio_de_atendimento_completo_do_paciente.txt
mkdir -p "$(dirname "$LONG")"
: >"$LONG"

ADDRESS="https://example.com/caminho/da/pagina?q=1"
# Long enough to need a second row at the window's width (84 to 144
# columns), short enough that the rest and the words after it fit in one.
LONG_ADDRESS="https://github.com/ttwwnn/keep/blob/main/apps/macos/Sources/Keep/Model/LinkOpener.swift?plain=1&utm_source=keep&utm_medium=terminal&utm_campaign=links-web#L120"

# The tab's program: fills the screen with the path and waits.
cat >"$WORK/printer" <<PY
#!/usr/bin/env python3
import sys, time
if "$MOUSE_MODE" == "1":
    sys.stdout.write("\x1b[?1000h\x1b[?1002h\x1b[?1006h")
if "$PRINT" == "pasted":
    # Claude Code's box after Ctrl+V (^V, which ⌘⇧V sends): the chip, with
    # no link and no file behind it yet.
    import os, signal, threading, tty
    if "$MOUSE_MODE" == "1":
        sys.stdout.write("\x1b[?1000h\x1b[?1002h\x1b[?1006h")
    tty.setraw(sys.stdin.fileno())
    pasted = []
    def draw(*_):
        if not pasted:
            return
        cols, rows = os.get_terminal_size()
        line = "  " + "[Image #3] " * max(1, (cols - 4) // 11)
        sys.stdout.write("\x1b[H\x1b[2J" + "\r\n".join([line] * (rows - 1)))
        sys.stdout.flush()
    def listen():
        while True:
            if b"\x16" in os.read(sys.stdin.fileno(), 1024):
                pasted.append(1)
                draw()
    signal.signal(signal.SIGWINCH, draw)
    threading.Thread(target=listen, daemon=True).start()
if "$MOUSE_MODE" == "1" and "$PRINT" == "image":
    # What reaches the program: a press arrives as ESC [ < 0 ; x ; y M.
    import os, threading, tty
    tty.setraw(sys.stdin.fileno())
    def listen():
        with open("$WORK/mouse", "ab", buffering=0) as log:
            while True:
                log.write(os.read(sys.stdin.fileno(), 1024))
    threading.Thread(target=listen, daemon=True).start()
if "$PRINT" == "wrapped":
    # Broken by the program, the rest indented on the next row: to the
    # terminal two words, the second without a slash for the runtime to see
    # a path in.
    for _ in range(20):
        print("  " + "$LONG"[:-44])
        print("  " + "$LONG"[-44:] + ":12")
elif "$PRINT" == "pasted":
    pass  # drawn on ^V, above
elif "$PRINT" == "image":
    import os, signal
    def draw(*_):
        # The chip of a pasted picture, as Claude Code draws it, across
        # every row: the text, with the file behind it.
        cols, rows = os.get_terminal_size()
        link = "\x1b]8;;file://$IMAGE\x07[Image #1]\x1b]8;;\x07 "
        line = "  " + link * max(1, (cols - 4) // 11)
        sys.stdout.write("\x1b[H\x1b[2J" + "\r\n".join([line] * (rows - 1)))
        sys.stdout.flush()
    signal.signal(signal.SIGWINCH, draw)
    draw()
elif "$PRINT" in ("web", "osc8", "webwrapped"):
    import os, signal
    def draw(*_):
        cols, rows = os.get_terminal_size()
        out = ["\x1b[H\x1b[2J"]
        for row in range(rows - 1):
            if "$PRINT" == "web":
                # One address filling the row, so any click on it lands on it.
                out.append("  " + ("$ADDRESS" + "&p=" + "x" * cols)[:cols - 4])
            elif "$PRINT" == "osc8":
                # A markdown link as Claude Code draws it: the text, with
                # the address behind it.
                text = ("documentação " * cols)[:cols - 4]
                out.append("  \x1b]8;id=d%d;$ADDRESS\x07" % row + text + "\x1b]8;;\x07")
            else:
                # Broken by the program at two columns short of the edge, the
                # rest indented on the next rows. A row written past the edge
                # would be wrapped again by the terminal and shift every row
                # under it (84 columns: "0 e também" on a row of its own,
                # 14/10/2026).
                width = cols - 4
                pieces = [("  " + "$LONG_ADDRESS"[i:i + width]) for i in range(0, len("$LONG_ADDRESS"), width)]
                # Every row a piece of the address, so any click on the
                # rows lands on it: the sentence only where it fits.
                if len(pieces[-1]) + len(" e também") <= cols - 2:
                    pieces[-1] += " e também"
                out.append(pieces[row % len(pieces)])
            out.append("\r\n")
        sys.stdout.write("".join(out))
        sys.stdout.flush()
    signal.signal(signal.SIGWINCH, draw)
    draw()
else:
    import os, signal
    def draw(*_):
        # As many copies as fit in a row: a row the terminal wraps comes back
        # joined to the next, and the click lands on the blank it leaves.
        cols, rows = os.get_terminal_size()
        unit = "$TARGET:12  "
        line = (unit * max(1, (cols - 1) // len(unit))).rstrip()
        sys.stdout.write("\x1b[H\x1b[2J" + "\r\n".join([line] * (rows - 1)))
        sys.stdout.flush()
    signal.signal(signal.SIGWINCH, draw)
    draw()
open("$WORK/ready", "w").close()
time.sleep(300)
PY
chmod +x "$WORK/printer"

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
TX=$(( ${FX%.*} + ${FW%.*} * 2 / 3 ))
TY=$(( ${FY%.*} + 120 ))

if [ "$PRINT" = web ] || [ "$PRINT" = osc8 ]; then
    say ""
    say "a web address on the screen ($PRINT)"
    "$MOUSE" click "$TX" "$TY"
    sleep 2
    if [ "$MOUSE_MODE" = 1 ]; then
        check "a plain click leaves it to the program, which has the mouse" "" "$(cat "$KEEP_LINK_LOG")"
    else
        check "a plain click opens it" "$ADDRESS" "$(head -1 "$KEEP_LINK_LOG" | cut -c1-${#ADDRESS})"
        check "once" "1" "$(wc -l <"$KEEP_LINK_LOG" | tr -d ' ')"
    fi
    : >"$KEEP_LINK_LOG"
    "$MOUSE" dblclick "$TX" "$((TY + 40))"
    sleep 2
    check "a double click opens nothing" "" "$(cat "$KEEP_LINK_LOG")"
    "$MOUSE" drag "$((TX - 60))" "$((TY + 20))" "$((TX + 60))" "$((TY + 20))" 10 20
    sleep 2
    check "a drag across it opens nothing" "" "$(cat "$KEEP_LINK_LOG")"
    "$MOUSE" move "$((TX - 150))" "$((TY + 60))"
    "$MOUSE" cmdclick "$TX" "$((TY + 80))"
    sleep 2
    check "a click with ⌘ opens it" "$ADDRESS" "$(head -1 "$KEEP_LINK_LOG" | cut -c1-${#ADDRESS})"
    : >"$KEEP_LINK_LOG"
    "$MOUSE" rightclick "$TX" "$((TY + 120))"
    sleep 1
    ITEMS=$("$AXPRESS" "$APP_NAME" items 2>/dev/null | cut -f3 | grep -E '^(Abrir link|Copiar link|Abrir|Mostrar no Finder)$' | tr '\n' '|')
    check "the right button offers the link's actions" "Abrir link|Copiar link|" "$ITEMS"
    "$AXPRESS" "$APP_NAME" pick "Abrir link" >/dev/null 2>&1
    sleep 1
    check "and Abrir link opens it" "$ADDRESS" "$(head -1 "$KEEP_LINK_LOG" | cut -c1-${#ADDRESS})"
    stop_app
    say ""
    say "passed $PASSED, failed $FAILED"
    [ "$FAILED" -eq 0 ]
    exit
fi

if [ "$PRINT" = image ]; then
    say ""
    say "a pasted picture on the screen"
    # The reports come with no newline between them: counted one by one.
    presses() { grep -ao '\[<0;[0-9]*;[0-9]*M' "$WORK/mouse" 2>/dev/null | wc -l | tr -d ' '; }
    # The column may fall on the blank between two chips: a few cells on is
    # one.
    for _ in 1 2 3; do
        "$MOUSE" click "$TX" "$TY"
        sleep 2
        [ -s "$KEEP_LINK_LOG" ] && break
        TX=$((TX + 24))
    done
    check "a plain click opens the picture" "$IMAGE" "$(cat "$KEEP_LINK_LOG")"
    if [ "$MOUSE_MODE" = 1 ]; then
        check "and the program, which has the mouse, never sees the press" "0" "$(presses)"
    fi
    : >"$KEEP_LINK_LOG"
    "$MOUSE" dblclick "$TX" "$((TY + 40))"
    sleep 2
    check "a double click opens nothing" "" "$(cat "$KEEP_LINK_LOG")"
    before=$(presses)
    "$MOUSE" drag "$((TX - 60))" "$((TY + 20))" "$((TX + 60))" "$((TY + 20))" 10 20
    sleep 2
    check "a drag across it opens nothing" "" "$(cat "$KEEP_LINK_LOG")"
    if [ "$MOUSE_MODE" = 1 ]; then
        check "and its press reaches the program" "yes" "$([ "$(presses)" -gt "$before" ] && echo yes || echo no)"
    fi
    stop_app
    say ""
    say "passed $PASSED, failed $FAILED"
    [ "$FAILED" -eq 0 ]
    exit
fi

if [ "$PRINT" = pasted ]; then
    say ""
    say "a picture in the box being typed"
    SENDKEY=$WORK/sendkey
    swiftc -O tools/sendkey.swift -o "$SENDKEY" 2>/dev/null || { say "could not build sendkey"; exit 1; }
    SAVED_CLIP=$(pbpaste)
    python3 -c 'import zlib,struct,sys
def c(t,d): return struct.pack(">I",len(d))+t+d+struct.pack(">I",zlib.crc32(t+d))
sys.stdout.buffer.write(b"\x89PNG\r\n\x1a\n"+c(b"IHDR",struct.pack(">IIBBBBB",1,1,8,2,0,0,0))+c(b"IDAT",zlib.compress(b"\0\xff\0\0"))+c(b"IEND",b""))' >"$WORK/colada.png"
    osascript -e "set the clipboard to (read (POSIX file \"$WORK/colada.png\") as «class PNGf»)" >/dev/null
    "$SENDKEY" "$APP_PID" key 9 cmd shift
    sleep 2.5
    printf %s "$SAVED_CLIP" | pbcopy
    for _ in 1 2 3; do
        "$MOUSE" click "$TX" "$TY"
        sleep 2
        [ -s "$KEEP_LINK_LOG" ] && break
        TX=$((TX + 24))
    done
    OPENED=$(cat "$KEEP_LINK_LOG")
    check "a plain click opens the copy kept at the paste" "yes" \
        "$(case "$OPENED" in */keep-coladas/*.png) [ -f "$OPENED" ] && echo yes || echo "no: $OPENED";; *) echo "no: $OPENED";; esac)"
    : >"$KEEP_LINK_LOG"
    "$MOUSE" dblclick "$TX" "$((TY + 40))"
    sleep 2
    check "a double click opens nothing" "" "$(cat "$KEEP_LINK_LOG")"
    stop_app
    say ""
    say "passed $PASSED, failed $FAILED"
    [ "$FAILED" -eq 0 ]
    exit
fi

if [ "$PRINT" = webwrapped ]; then
    say ""
    say "a web address broken across rows"
    LX=$(( ${FX%.*} + 420 ))
    for step in 0 1 2 3 4 5; do
        "$MOUSE" move "$((LX - 150))" "$((TY + step * 7 + 60))"
        "$MOUSE" click "$LX" "$((TY + step * 7))"
        sleep 1.5
    done
    sleep 1
    if [ "$MOUSE_MODE" = 1 ]; then
        check "every plain click leaves it to the program, which has the mouse" "" "$(cat "$KEEP_LINK_LOG")"
    else
        check "every plain click opens the whole address" "6 $LONG_ADDRESS" \
            "$(wc -l <"$KEEP_LINK_LOG" | tr -d ' ') $(sort -u "$KEEP_LINK_LOG")"
    fi
    : >"$KEEP_LINK_LOG"
    for step in 0 1 2 3; do
        "$MOUSE" move "$((LX - 150))" "$((TY + step * 7 + 100))"
        "$MOUSE" cmdclick "$LX" "$((TY + step * 7 + 40))"
        sleep 1.5
    done
    sleep 1
    check "every ⌘-click opens the whole address" "4 $LONG_ADDRESS" \
        "$(wc -l <"$KEEP_LINK_LOG" | tr -d ' ') $(sort -u "$KEEP_LINK_LOG")"
    stop_app
    say ""
    say "passed $PASSED, failed $FAILED"
    [ "$FAILED" -eq 0 ]
    exit
fi

if [ "$PRINT" = wrapped ]; then
    # Six clicks a few points apart, so rows of both kinds get some.
    LX=$(( ${FX%.*} + 420 ))
    for step in 0 1 2 3 4 5; do
        "$MOUSE" move "$((LX - 150))" "$((TY + step * 7 + 60))"
        "$MOUSE" cmdclick "$LX" "$((TY + step * 7))"
        sleep 1
    done
    sleep 1
    check "every click opens the whole path" "6 $LONG" \
        "$(wc -l <"$KEEP_LINK_LOG" | tr -d ' ') $(sort -u "$KEEP_LINK_LOG")"
    check "the row without a slash was among them" "yes" \
        "$(grep -q 'no link under the click; the word: .*_paciente' "$WORK/app.log" && echo yes || echo no)"
    stop_app
    say ""
    say "passed $PASSED, failed $FAILED"
    [ "$FAILED" -eq 0 ]
    exit
fi

say ""
say "a path on the screen"
# Every row is the same, so a column on a path is one on every row. The
# first column tried may fall on the blank between two copies (the app says
# so: "plain click on nothing"), and a click there rightly opens nothing; a
# few cells on is a path.
for _ in 1 2 3; do
    before=$(grep -c 'plain click on nothing' "$WORK/app.log")
    "$MOUSE" click "$TX" "$TY"
    sleep 2
    [ -s "$KEEP_LINK_LOG" ] && break
    [ "$(grep -c 'plain click on nothing' "$WORK/app.log")" -gt "$before" ] || break
    say "  (column on the blank between two copies; moving 3 cells on)"
    TX=$((TX + 24))
done
check "a plain click opens the file" "$TARGET" "$(cat "$KEEP_LINK_LOG")"
: >"$KEEP_LINK_LOG"
"$MOUSE" dblclick "$TX" "$((TY + 40))"
sleep 2
check "a double click opens nothing" "" "$(cat "$KEEP_LINK_LOG")"
"$MOUSE" drag "$((TX - 60))" "$TY" "$((TX + 60))" "$TY" 10 20
sleep 2
check "a drag across it opens nothing" "" "$(cat "$KEEP_LINK_LOG")"
osascript -e 'tell application "Finder" to activate' >/dev/null 2>&1
sleep 1
"$MOUSE" click "$TX" "$((TY + 80))"
sleep 2
check "the click that brings the window forward opens nothing" "" "$(cat "$KEEP_LINK_LOG")"
"$MOUSE" click "$TX" "$((TY + 80))"
sleep 2
check "the next one opens it" "$TARGET" "$(cat "$KEEP_LINK_LOG")"
: >"$KEEP_LINK_LOG"
"$MOUSE" move "$((TX - 150))" "$((TY + 60))"
"$MOUSE" cmdclick "$TX" "$TY"
sleep 2
check "a click with ⌘ opens the file, the :line left off" "$TARGET" "$(head -1 "$KEEP_LINK_LOG")"


say ""
say "the pointer already resting on the path, ⌘ pressed after"
: >"$KEEP_LINK_LOG"
"$MOUSE" move "$((TX - 150))" "$((TY + 120))"
"$MOUSE" move "$TX" "$((TY + 120))"
sleep 1
"$MOUSE" cmdkey down
"$MOUSE" cmdclick "$TX" "$((TY + 120))"
"$MOUSE" cmdkey up
sleep 2
check "the click opens the file without the pointer having moved since ⌘ went down" \
    "$TARGET" "$(head -1 "$KEEP_LINK_LOG")"

stop_app
say ""
say "passed $PASSED, failed $FAILED"
[ "$FAILED" -eq 0 ]
