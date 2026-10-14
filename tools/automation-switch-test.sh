#!/usr/bin/env bash
#
# The footer's two switches, the Jev and the Ponytail, from outside the app.
#
# The Jev decides each message's model, effort and whether it takes the
# Ponytail's rules; its hooks do nothing while ~/.claude/jev/desligado is
# there. The Ponytail is the plugin ponytail@ponytail in Claude
# (`enabledPlugins` in ~/.claude/settings.json, changed by `claude plugin
# enable|disable`) and in Codex (`[plugins."ponytail@ponytail"] enabled` in
# ~/.codex/config.toml). What this checks:
#   - both switches are under the footer, each saying whether it is on;
#   - the Jev's switch writes and removes the Jev's "off" file;
#   - the Ponytail's switch turns the plugin off and on in both AIs, through
#     Claude Code itself and the one line of Codex's file;
#   - a switch turned in a terminal (`jev auto desligar`) shows in the footer
#     within seconds;
#   - a refusal is said, and the switch goes back to what the AI has;
#   - what is not installed has no switch.
#
#   tools/automation-switch-test.sh
#
# A home of the test's own, a `claude` that edits only that home, a daemon of
# its own on a scratch socket, KeepDev, read and pressed through the
# accessibility tree (tools/axtext.swift, tools/axpress.swift). About a minute.
#
# Your app, your daemon, your Jev and your plugins are never touched.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=$(dev_app)
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
SOCKET=/tmp/keep-auto-$$.sock
WORK=$(mktemp -d /tmp/keep-auto-XXXXXX)
AXTEXT=$WORK/axtext
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
# value <switch>: "ligado" or "desligado" as the switch says it (1 or 0), "-"
# when there is none.
value() {
    case "$("$AXPRESS" "$APP_NAME" value "automation-$1" 2>/dev/null)" in
        1|ligado) echo ligado ;;
        0|desligado) echo desligado ;;
        *) echo - ;;
    esac
}
# until_value <switch> <wanted>: wait up to 8 s for the switch to say it.
until_value() {
    local waited=0 got
    got=$(value "$1")
    while [ "$got" != "$2" ] && [ "$waited" -lt 32 ]; do
        sleep 0.25; waited=$((waited + 1)); got=$(value "$1")
    done
    printf '%s' "$got"
}
# until_true <shell test>: wait up to 8 s for it.
until_true() {
    local waited=0
    while ! eval "$1" && [ "$waited" -lt 32 ]; do sleep 0.25; waited=$((waited + 1)); done
    eval "$1" && echo yes || echo no
}
plugin_in_claude() { python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("enabledPlugins",{}).get("ponytail@ponytail"))' "$FAKE/.claude/settings.json"; }
plugin_in_codex() { grep -A1 -F '[plugins."ponytail@ponytail"]' "$FAKE/.codex/config.toml" | tail -1; }

if [ -z "${SKIP_BUILD:-}" ]; then
    say "building $APP_NAME, the client, the daemon and the tools (yours is left alone)"
    ./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }
fi
swiftc -O tools/axtext.swift -o "$AXTEXT" 2>/dev/null || { say "could not build axtext"; exit 1; }
swiftc -O tools/axpress.swift -o "$AXPRESS" 2>/dev/null || { say "could not build axpress"; exit 1; }

# ------------------------------------------------------------ a made-up home
FAKE=$WORK/home
mkdir -p "$FAKE/.claude/jev" "$FAKE/.codex"
cat >"$FAKE/.claude/settings.json" <<'JSON'
{
  "model": "opus[1m]",
  "enabledPlugins": {
    "ponytail@ponytail": true
  }
}
JSON
cat >"$FAKE/.codex/config.toml" <<'TOML'
model = "gpt-6-astra"

[plugins."ponytail@ponytail"]
enabled = true

[projects."/x"]
trust_level = "trusted"
TOML
cp "$FAKE/.codex/config.toml" "$WORK/config.toml.before"
# Claude Code as the core calls it for the plugin: it changes enabledPlugins in
# the made-up home and nothing else; with $WORK/claude-fails there, it fails.
cat >"$WORK/claude" <<SH
#!/bin/sh
echo "\$@" >> "$WORK/claude.log"
[ -f "$WORK/claude-fails" ] && { echo "sem rede para o marketplace" >&2; exit 1; }
[ "\$2" = enable ] && V=true || V=false
sed -i '' "s/\"ponytail@ponytail\": *[a-z]*/\"ponytail@ponytail\": \$V/" "$FAKE/.claude/settings.json"
SH
chmod +x "$WORK/claude"

export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$WORK/state
export KEEP_AI_USAGE_HOME=$FAKE
export KEEP_IA_CLAUDE_BIN=$WORK/claude
# The core, the real one in the bundle (scratch.sh points the tests at none).
export KEEP_IA_BIN=$APP/Contents/Resources/keep
mkdir -p "$KEEP_STATE_DIR"
defaults write dev.luhw.keep.dev usageFooterFolded -bool false 2>/dev/null
require_scratch_socket
rm -f "$SOCKET"

"$APP/Contents/Resources/keepd" >>"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
waited=0
while [ ! -S "$SOCKET" ] && [ "$waited" -lt 20 ]; do sleep 0.25; waited=$((waited + 1)); done

stop_app
KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
APP_PID=$!
waited=0
while ! grep -aqF "keep ia automacao" "$WORK/app.log" && [ "$waited" -lt 120 ]; do
    sleep 0.25; waited=$((waited + 1))
done
place_on_screen

say ""
say "both switches, each saying whether it is on"
check "the Jev's switch, on" ligado "$(until_value jev ligado)"
check "the Ponytail's switch, on" ligado "$(until_value ponytail ligado)"
"$AXTEXT" "$APP_NAME" >"$WORK/ax.txt" 2>/dev/null
check "named in the footer" yes "$(grep -qF "Ponytail" "$WORK/ax.txt" && echo yes || echo no)"

say ""
say "the Jev's switch is the Jev's off file"
"$AXPRESS" "$APP_NAME" press automation-jev >/dev/null 2>&1
check "pressed, the file the hooks read is written" yes "$(until_true "[ -f '$FAKE/.claude/jev/desligado' ]")"
check "and the switch says off" desligado "$(until_value jev desligado)"
"$AXPRESS" "$APP_NAME" press automation-jev >/dev/null 2>&1
check "pressed again, the file is gone" yes "$(until_true "[ ! -e '$FAKE/.claude/jev/desligado' ]")"
check "and the switch says on" ligado "$(until_value jev ligado)"

say ""
say "the Ponytail's switch is the plugin in both AIs"
"$AXPRESS" "$APP_NAME" press automation-ponytail >/dev/null 2>&1
check "off in Claude, by Claude Code itself" yes "$(until_true "[ \"\$(plugin_in_claude)\" = False ]")"
check "off in Codex" "enabled = false" "$(plugin_in_codex)"
check "Claude Code was asked once" "plugin disable ponytail@ponytail" "$(cat "$WORK/claude.log" 2>/dev/null)"
check "the switch says off" desligado "$(until_value ponytail desligado)"
"$AXPRESS" "$APP_NAME" press automation-ponytail >/dev/null 2>&1
check "on again in Claude" yes "$(until_true "[ \"\$(plugin_in_claude)\" = True ]")"
check "and Codex's file is as it was, line for line" yes "$(until_true "cmp -s '$FAKE/.codex/config.toml' '$WORK/config.toml.before'")"
check "the switch says on" ligado "$(until_value ponytail ligado)"

say ""
say "a switch turned in a terminal shows"
date >"$FAKE/.claude/jev/desligado"   # what `jev auto desligar` writes
check "the Jev turned off outside the app" desligado "$(until_value jev desligado)"
rm -f "$FAKE/.claude/jev/desligado"
check "and back on" ligado "$(until_value jev ligado)"

say ""
say "a refusal is said and taken back"
touch "$WORK/claude-fails"
rm -f "$FAKE/.codex/config.toml"   # only Claude has it now: nothing else can turn it off
check "the Ponytail, in Claude only, still a switch" ligado "$(until_value ponytail ligado)"
"$AXPRESS" "$APP_NAME" press automation-ponytail >/dev/null 2>&1
sleep 1
"$AXTEXT" "$APP_NAME" >"$WORK/ax.txt" 2>/dev/null
check "the footer says why" yes "$(grep -qF "Não deu para desligar o Ponytail" "$WORK/ax.txt" && echo yes || echo no)"
check "and the switch is back on, as Claude has it" ligado "$(until_value ponytail ligado)"
rm -f "$WORK/claude-fails"

say ""
say "what is not installed has no switch"
printf '{\n  "model": "opus[1m]"\n}\n' >"$FAKE/.claude/settings.json"
check "the Ponytail gone from both AIs, its switch goes" - "$(until_value ponytail -)"
check "the Jev's stays" ligado "$(value jev)"

say ""
say "$PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
