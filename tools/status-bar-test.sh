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


# The tab's program: poses as Claude Code, its two pickers included (as they
# were on 12/10: `/model`'s list and `/effort`'s rule, each taking "s" for
# this session only), and writes down every key it is sent.
cat >"$WORK/printer" <<'PY'
#!/usr/bin/env python3
import json, os, sys, tty
WORK = os.environ["KEEP_BAR_WORK"]
tty.setraw(0)
out = sys.stdout
LEVELS = ["low", "medium", "high", "xhigh", "max"]
MODELS = [("Default (recommended)", None), ("Opus 5.5", ("claude-opus-5-5", "Opus 5.5")),
          ("Fable 5.1", ("claude-fable-5-1", "Fable 5.1")), ("Sonnet 5.5", ("claude-sonnet-5-5", "Sonnet 5.5")),
          ("Haiku 5.5", ("claude-haiku-5-5", "Haiku 5.5"))]
effort = os.environ.get("BAR_EFFORT", "xhigh")
model = ("claude-opus-5-5[1m]", "Opus 5.5 (1M context)")
screen, typed, pointer, marker = "box", "", 1, LEVELS.index(effort)
def status():
    # For the tab's shell, which is the program above this one (see below).
    import subprocess
    pids = {os.getppid()}
    pids.add(int(subprocess.check_output(["ps", "-o", "ppid=", "-p", str(os.getppid())]).split()[0]))
    for pid in pids:
        with open(os.path.join(os.environ["KEEP_STATUS_DIR"], "%d.json" % pid), "w") as f:
            json.dump({"model": {"id": model[0], "display_name": model[1]}, "effort": {"level": effort}}, f)
def draw():
    # As wide as the terminal, as Claude Code draws it: a fixed width wraps
    # in a narrower pane, and the box is not found.
    rule = "─" * os.get_terminal_size(0).columns
    out.write("\x1b]2;✳ teste\x07\x1b[2J\x1b[H")
    if screen == "models":
        out.write("   Select model\r\n")
        for i, (label, _m) in enumerate(MODELS):
            out.write(("   ❯ " if i == pointer else "     ") + "%d.  %s\r\n" % (i + 1, label))
        out.write("   Enter to set as default · s to use this session only · Esc to cancel\r\n")
    elif screen == "effort":
        cols = [29, 37, 48, 57, 68]
        line = [" "] * 29 + ["─"] * 42
        line[cols[marker] + 1] = "▲"
        out.write("   Effort\r\n" + "".join(line) + "\r\n")
        out.write(" " * 29 + "low     medium     high     xhigh      max\r\n")
        out.write("   ←/→ to adjust · Enter to confirm · s for this session only · Esc to cancel\r\n")
    else:
        out.write("\r\n" * 3 + rule + "\r\n❯ " + typed + "\r\n" + rule + "\r\n")
        out.write("  ⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents\r\n\r\n")
        out.write("  ⏺ main\r\n  ◯ general-purpose  Reading files\r\n  ◯ Explore  Scanning\r\n  ◯ Plan  Thinking\r\n")
        out.write("\x1b[5;%dH" % (3 + len(typed)))
    out.flush()
status()
draw()
open(os.path.join(WORK, "ready"), "w").close()
raw = b""
while True:
    data = os.read(0, 256)
    if not data:
        break
    # The test empties the file to start a new count; what came before is
    # not carried into the next check.
    mark = os.path.join(WORK, "bytes")
    if raw and os.path.exists(mark) and os.path.getsize(mark) == 0:
        raw = b""
    raw += data
    with open(os.path.join(WORK, "bytes"), "w") as f:
        f.write(raw.decode("utf-8", "replace").replace("\r", "<CR>").replace("\x1b", "<ESC>"))
    text = data.decode("utf-8", "replace")
    i = 0
    while i < len(text):
        seq = text[i:i + 3]
        if seq in ("\x1b[A", "\x1b[B", "\x1b[C", "\x1b[D"):
            if screen == "models":
                pointer = max(0, min(len(MODELS) - 1, pointer + (1 if seq == "\x1b[B" else -1 if seq == "\x1b[A" else 0)))
            elif screen == "effort":
                marker = max(0, min(4, marker + (1 if seq == "\x1b[C" else -1 if seq == "\x1b[D" else 0)))
            i += 3
            continue
        ch = text[i]
        i += 1
        if ch == "\x1b":
            screen, typed = "box", ""
        elif screen == "box":
            if ch == "\r":
                if typed == "/model":
                    screen = "models"
                elif typed == "/effort":
                    screen = "effort"
                elif typed.startswith("/model "):
                    model = (typed[7:], typed[7:])
                    status()
                typed = ""
            else:
                typed += ch
        elif ch == "s" and screen == "models" and MODELS[pointer][1]:
            model, screen = MODELS[pointer][1], "box"
            status()
        elif ch == "s" and screen == "effort":
            effort, screen = LEVELS[marker], "box"
            status()
    draw()
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
for _ in $(seq 20); do [ "$(bar claude-effort title)" = "esforço max ▾" ] && break; sleep 1; done
check "Claude's effort picker is walked: one right, then s (this session only)" "/effort<CR><ESC>[Cs" "$(cat "$WORK/bytes" 2>/dev/null)"
check "the bar follows" "esforço max ▾" "$(bar claude-effort title)"

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
: > "$WORK/bytes"
"$AXPRESS" "$APP_NAME" pick "Sonnet 5.5" >/dev/null 2>&1
for _ in $(seq 20); do [ "$(bar claude-model title)" = "Sonnet 5.5 ▾" ] && break; sleep 1; done
check "Claude's model picker is walked: down to Sonnet 5.5, then s" "/model<CR><ESC>[B<ESC>[Bs" "$(cat "$WORK/bytes" 2>/dev/null)"
check "the bar follows" "Sonnet 5.5 ▾" "$(bar claude-model title)"

say ""
say "the 1M-context Opus, which the picker does not list"
ITEMS=
for _ in 1 2 3; do
    "$AXPRESS" "$APP_NAME" open claude-model >/dev/null 2>&1
    sleep 1
    ITEMS=$("$AXPRESS" "$APP_NAME" items 2>/dev/null | cut -f3 | tr '\n' '|')
    [ -n "$ITEMS" ] && break
done
: > "$WORK/bytes"
"$AXPRESS" "$APP_NAME" pick "Opus 5.5 (1M)" >/dev/null 2>&1
for _ in $(seq 20); do [ "$(bar claude-model title)" = "claude-opus-5-5[1m] ▾" ] && break; sleep 1; done
check "it is asked for by its id" "/model claude-opus-5-5[1m]<CR>" "$(cat "$WORK/bytes" 2>/dev/null)"

stop_app
say ""
say "passed $PASSED, failed $FAILED"
[ "$FAILED" -eq 0 ]
