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
#   - web addresses, the mouse taken as Claude Code takes it (PRINT=web): a
#     plain click on one opens it in the browser, a double click does not;
#     a markdown link whose address is behind its text (OSC 8) opens that
#     address; one broken across rows opens whole, plain click or ⌘-click;
#     the right button offers "Abrir link".
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
        echo "== web addresses ($print), the mouse taken"
        MOUSE_MODE=1 PRINT=$print "$0" || status=1
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
if "$PRINT" == "wrapped":
    # Broken by the program, the rest indented on the next row: to the
    # terminal two words, the second without a slash for the runtime to see
    # a path in.
    for _ in range(20):
        print("  " + "$LONG"[:-44])
        print("  " + "$LONG"[-44:] + ":12")
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
                # rest indented on the next row, then the sentence goes on.
                first = cols - 4
                if row % 2 == 0:
                    out.append("  " + "$LONG_ADDRESS"[:first])
                else:
                    out.append("  " + "$LONG_ADDRESS"[first:] + " e também")
            out.append("\r\n")
        sys.stdout.write("".join(out))
        sys.stdout.flush()
    signal.signal(signal.SIGWINCH, draw)
    draw()
else:
    for _ in range(40):
        print(("$TARGET:12  " * 6).rstrip())
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
TX=$(( ${FX%.*} + ${FW%.*} * 2 / 3 ))
TY=$(( ${FY%.*} + 120 ))

if [ "$PRINT" = web ] || [ "$PRINT" = osc8 ]; then
    say ""
    say "a web address on the screen ($PRINT)"
    "$MOUSE" click "$TX" "$TY"
    sleep 2
    check "a plain click opens it" "$ADDRESS" "$(head -1 "$KEEP_LINK_LOG" | cut -c1-${#ADDRESS})"
    check "once" "1" "$(wc -l <"$KEEP_LINK_LOG" | tr -d ' ')"
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
    check "every plain click opens the whole address" "6 $LONG_ADDRESS" \
        "$(wc -l <"$KEEP_LINK_LOG" | tr -d ' ') $(sort -u "$KEEP_LINK_LOG")"
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
"$MOUSE" click "$TX" "$TY"
sleep 2
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
