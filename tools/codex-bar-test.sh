#!/usr/bin/env bash
#
# The bar under a Codex conversation: the model and the effort Codex shows
# under its prompt, and a click on either changing them through Codex's own
# picker (`/model`, the model's number, the level's, by way of "More
# reasoning…" for Max and Ultra).
#
# The tab's program poses as Codex 0.162: its screens are the ones it drew on
# 12/10 (prompt, footer, the two pickers and the advanced one), it changes
# model and effort when chosen, and writes down every key it is sent. The bar
# is read and its menus picked through the accessibility tree.
#
#   tools/codex-bar-test.sh
#
# Your app, your daemon and your Codex are never touched.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=$(dev_app)
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
SOCKET=/tmp/keep-codexbar-$$.sock
WORK=$(mktemp -d /tmp/keep-codexbar-XXXXXX)
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

# Codex's model cache, made up: what the bar offers is read from it.
export CODEX_HOME=$WORK/codex-home
mkdir -p "$CODEX_HOME"
cat >"$CODEX_HOME/models_cache.json" <<'JSON'
{"models": [
 {"slug": "gpt-6.1-sol", "display_name": "GPT-6.1-Sol", "visibility": "list", "priority": 1,
  "default_reasoning_level": "low",
  "supported_reasoning_levels": [{"effort": "low"}, {"effort": "medium"}, {"effort": "high"}, {"effort": "xhigh"}]},
 {"slug": "gpt-6-astra", "display_name": "GPT-6-Astra", "visibility": "list", "priority": 2,
  "default_reasoning_level": "medium",
  "supported_reasoning_levels": [{"effort": "low"}, {"effort": "medium"}, {"effort": "high"}, {"effort": "xhigh"}, {"effort": "max"}, {"effort": "ultra"}]},
 {"slug": "gpt-6-sol", "display_name": "GPT-6-Sol", "visibility": "list", "priority": 3,
  "default_reasoning_level": "medium",
  "supported_reasoning_levels": [{"effort": "low"}, {"effort": "medium"}, {"effort": "high"}, {"effort": "xhigh"}, {"effort": "max"}, {"effort": "ultra"}]}
]}
JSON


# The tab's program: poses as Codex.
cat >"$WORK/printer" <<'PY'
#!/usr/bin/env python3
import os, sys, tty
WORK = os.environ["KEEP_BAR_WORK"]
tty.setraw(0)
out = sys.stdout
MODELS = ["GPT-6.1-Sol", "GPT-6-Astra", "GPT-6-Sol"]
LEVELS = ["low", "medium", "high", "xhigh"]
model, effort, screen, picked, typed = "GPT-6-Astra", "max", "idle", None, ""
said = []
def draw():
    out.write("\x1b[2J\x1b[H\r\n  >_ OpenAI Codex (v0.162.1)\r\n     ~/x\r\n\r\n")
    if screen == "idle":
        for line in said[-3:]:
            out.write(line + "\r\n\r\n")
        out.write("› " + (typed or "Ask Codex to do anything") + "\r\n\r\n")
        out.write("  %s %s · ~/x\r\n  ← for agents · ? for shortcuts\r\n" % (model, effort))
    elif screen == "models":
        out.write("  Select Model and Effort\r\n\r\n")
        for i, m in enumerate(MODELS, 1):
            cur = m == model
            out.write(("› " if cur else "  ") + "%d. %s%s\r\n" % (i, m, " (current)" if cur else ""))
        out.write("\r\n  enter select · esc back\r\n")
    elif screen == "levels":
        out.write("  Select Reasoning Level for %s\r\n\r\n" % picked)
        for i, l in enumerate(["Low", "Medium", "High", "Extra high", "More reasoning…"], 1):
            out.write("  %d. %s\r\n" % (i, l))
        out.write("\r\n  enter select · esc back\r\n")
    elif screen == "advanced":
        out.write("  Advanced Reasoning\r\n  ⚠ Consumes usage limits faster\r\n\r\n")
        out.write("  1. Max\r\n  2. Ultra\r\n\r\n  enter default · s session · esc back\r\n")
    out.flush()
def change(m, e):
    global model, effort, screen
    model, effort, screen = m, e, "idle"
    said.append("• Model changed to %s %s" % (m.lower(), e))
draw()
open(os.path.join(WORK, "ready"), "w").close()
while True:
    data = os.read(0, 256)
    if not data:
        break
    with open(os.path.join(WORK, "bytes"), "a") as f:
        f.write(data.decode("utf-8", "replace").replace("\r", "<CR>").replace("\x1b", "<ESC>"))
    for ch in data.decode("utf-8", "replace"):
        if ch == "\x1b":
            screen, typed = "idle", ""
        elif screen == "idle":
            if ch == "\r":
                if typed == "/model":
                    screen = "models"
                typed = ""
            else:
                typed += ch
        elif screen == "models" and ch.isdigit() and 1 <= int(ch) <= len(MODELS):
            picked, screen = MODELS[int(ch) - 1], "levels"
        elif screen == "levels" and ch.isdigit():
            n = int(ch)
            if 1 <= n <= 4:
                change(picked, LEVELS[n - 1])
            elif n == 5:
                screen = "advanced"
        elif screen == "advanced" and ch in "12":
            change(picked, "max" if ch == "1" else "ultra")
    draw()
PY
chmod +x "$WORK/printer"
export KEEP_BAR_WORK=$WORK
# Codex's process is called codex, which is how Keep knows what runs in the
# tab: a program of that name holds the terminal and runs the printer.
cat >"$WORK/fake.c" <<C
#include <stdlib.h>
int main(void) { return system("$WORK/printer"); }
C
cc -o "$WORK/codex" "$WORK/fake.c" || { say "could not build the stand-in"; exit 1; }

SHELL="$WORK/codex" ./target/release/keepd >"$WORK/daemon.log" 2>&1 &
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
wait_for() {  # wait_for <identifier> <title>
    for _ in $(seq 20); do
        [ "$(bar "$1" title)" = "$2" ] && return 0
        sleep 1
    done
    return 1
}
menu() {  # menu <identifier> -> the items of its menu, opened
    local items=
    for _ in 1 2 3; do
        "$AXPRESS" "$APP_NAME" open "$1" >/dev/null 2>&1
        sleep 1
        items=$("$AXPRESS" "$APP_NAME" items 2>/dev/null | cut -f3 | tr '\n' '|')
        [ -n "$items" ] && break
    done
    printf '%s' "$items"
}
wait_for claude-model "GPT-6-Astra ▾"
check "the model, as Codex shows it" "GPT-6-Astra ▾" "$(bar claude-model title)"
check "the effort" "esforço max ▾" "$(bar claude-effort title)"

say ""
say "clicking the effort"
check "it offers what the model takes" "low|medium|high|xhigh|max|ultra|" "$(menu claude-effort)"
: > "$WORK/bytes"
"$AXPRESS" "$APP_NAME" pick high >/dev/null 2>&1
wait_for claude-effort "esforço high ▾"
check "Codex's picker is walked: /model, the model's number, the level's" "/model<CR>23" "$(cat "$WORK/bytes")"
check "the bar follows" "esforço high ▾" "$(bar claude-effort title)"

say ""
say "clicking the model"
check "it offers Codex's models" "GPT-6.1-Sol|GPT-6-Astra|GPT-6-Sol|" "$(menu claude-model)"
: > "$WORK/bytes"
"$AXPRESS" "$APP_NAME" pick "GPT-6-Sol" >/dev/null 2>&1
wait_for claude-model "GPT-6-Sol ▾"
check "the model changes and the effort stays" "/model<CR>33" "$(cat "$WORK/bytes")"
check "the bar follows" "GPT-6-Sol ▾" "$(bar claude-model title)"

say ""
say "Ultra, under More reasoning"
menu claude-effort >/dev/null
: > "$WORK/bytes"
"$AXPRESS" "$APP_NAME" pick ultra >/dev/null 2>&1
wait_for claude-effort "esforço ultra ▾"
check "by way of More reasoning" "/model<CR>352" "$(cat "$WORK/bytes")"

stop_app
say ""
say "passed $PASSED, failed $FAILED"
[ "$FAILED" -eq 0 ]
