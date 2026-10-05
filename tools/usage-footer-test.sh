#!/usr/bin/env bash
#
# The AI usage footer, from outside the app.
#
# The footer shows what the core (`keep ia uso`, the `keep` inside the app)
# finds and measures: the logins this Mac has — Claude Code's own, the kit's
# vault in ~/.claude/contas, Codex's ~/.codex/auth.json — and how much of each
# account's allowance is spent, a bar per window at the bottom of the
# sidebar. What this checks:
#   - every account is there, one line per login (the global login and two
#     vault slots holding the same one are one account), under the name the
#     tabs know it by and with its address written out; Codex's single login
#     is its "principal";
#   - the bars carry the figures the service answered, for Claude and Codex;
#   - an account whose access has lapsed is not asked at all (nothing here
#     renews a token) and says so, and a "too many requests" answer is said;
#   - the requests carry the account's token and a CLI's user agent, and go
#     nowhere but the stand-in;
#   - the footer sits under the last workspace and stays on the floor when
#     the list outgrows the window;
#   - each account carries its place in the order and arrows to move it: the
#     order changes on screen at once, even with the core slow to answer, is
#     what the core then writes, and is taken back, and said, when the core
#     refuses; the arrows are there folded too; and "+" asks the core for a
#     login in this window's workspace, whose account shows up within
#     seconds — and stays, though a round of readings that set out before it
#     lands after;
#   - without a `keep` to ask, there is no footer.
#
#   tools/usage-footer-test.sh
#
# Nothing real is read or asked: a home of the test's own holds made-up
# logins, a stand-in Keychain holds its global one, and a stand-in on
# 127.0.0.1 answers in the services' own format — the only kind of address
# the core accepts in place of the real ones. The core is the real one
# (the bundle's `keep`), reached through the stand-in
# tools/ia-test/fake-keep-ia.py, which plays it only where a test needs to
# make it slow, refuse or open a login. A daemon of its own on a scratch
# socket, KeepDev, read through the accessibility tree (tools/axtext.swift,
# tools/axpress.swift). About two minutes.
#
# Your app, your daemon and your logins are never touched.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=$(dev_app)
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
SOCKET=/tmp/keep-usage-$$.sock
WORK=$(mktemp -d /tmp/keep-usage-XXXXXX)
AXTEXT=$WORK/axtext
AXPOS=$WORK/axpos
AXPRESS=$WORK/axpress
KEEP=$APP/Contents/Resources/keep   # the client and daemon the build put in the bundle
HOME_WS=$(id -un)
PASSED=0
FAILED=0

cleanup() {
    [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
    [ -n "${SERVER_PID:-}" ] && kill "$SERVER_PID" 2>/dev/null
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
has() {  # has <fixed text>: yes if some line of the window's text contains it
    grep -qF -- "$1" "$WORK/ax.txt" && echo yes || echo no
}

if [ -z "${SKIP_BUILD:-}" ]; then
    say "building $APP_NAME, the client, the daemon and the tools (yours is left alone)"
    ./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }
fi
swiftc -O tools/axtext.swift -o "$AXTEXT" 2>/dev/null || { say "could not build axtext"; exit 1; }
swiftc -O tools/axpress.swift -o "$AXPRESS" 2>/dev/null || { say "could not build axpress"; exit 1; }

# Where on screen a text is: the top of the first element carrying it, and
# the bottom of its window — enough to tell a footer on the floor from one
# that scrolled.
cat >"$WORK/axpos.swift" <<'SWIFT'
import AppKit
import ApplicationServices
let name = CommandLine.arguments[1], wanted = CommandLine.arguments[2]
guard let app = NSWorkspace.shared.runningApplications.first(where: { $0.localizedName == name })
else { exit(1) }
func attribute(_ e: AXUIElement, _ k: String) -> AnyObject? {
    var v: AnyObject?
    return AXUIElementCopyAttributeValue(e, k as CFString, &v) == .success ? v : nil
}
func point(_ e: AXUIElement, _ k: String) -> CGPoint {
    var p = CGPoint.zero
    if let v = attribute(e, k) { AXValueGetValue(v as! AXValue, .cgPoint, &p) }
    return p
}
func size(_ e: AXUIElement) -> CGSize {
    var s = CGSize.zero
    if let v = attribute(e, "AXSize") { AXValueGetValue(v as! AXValue, .cgSize, &s) }
    return s
}
var found: CGFloat?
func walk(_ e: AXUIElement, _ depth: Int) {
    guard found == nil, depth < 60 else { return }
    for k in ["AXValue", "AXTitle", "AXDescription"] {
        if let t = attribute(e, k) as? String, t.hasPrefix(wanted) { found = point(e, "AXPosition").y; return }
    }
    for c in attribute(e, "AXChildren") as? [AXUIElement] ?? [] { walk(c, depth + 1) }
}
let root = AXUIElementCreateApplication(app.processIdentifier)
guard let window = (attribute(root, "AXWindows") as? [AXUIElement])?.first else { exit(1) }
walk(window, 0)
let bottom = point(window, "AXPosition").y + size(window).height
print("\(Int(found ?? -1)) \(Int(bottom))")
SWIFT
swiftc -O "$WORK/axpos.swift" -o "$AXPOS" 2>/dev/null || { say "could not build axpos"; exit 1; }

# ------------------------------------------------------------ made-up logins
FAKE=$WORK/home
VAULT=$FAKE/.claude/contas
mkdir -p "$VAULT" "$FAKE/.codex"
python3 - "$FAKE" <<'PY'
import base64, json, os, sys, time
home = sys.argv[1]
vault = os.path.join(home, ".claude/contas")
now = time.time()
def slot(name, uuid, token, expires_in, email):
    body = {"apelido": name, "email": email, "accountUuid": uuid,
            "credenciais": {"claudeAiOauth": {
                "accessToken": token, "refreshToken": "never-used",
                "expiresAt": int((now + expires_in) * 1000),
                "subscriptionType": "max", "rateLimitTier": "default_claude_max_20x"}}}
    json.dump(body, open(os.path.join(vault, name + ".json"), "w"))
slot("principal", "U1", "tok-principal", 5 * 3600, "um@exemplo.com")
slot("assinaturas", "U1", "tok-principal-velho", 3600, "um@exemplo.com")   # same login, older copy
slot("reserva", "U2", "tok-vencido", -3600, "dois@exemplo.com")             # access lapsed
slot("limitada", "U3", "tok-429", 5 * 3600, "tres@exemplo.com")            # the service says wait
open(os.path.join(vault, ".ativa"), "w").write("principal\n")
open(os.path.join(vault, ".preferida"), "w").write("principal\n")
# Claude Code's own login, which the tabs run on: the principal's, in the
# stand-in Keychain below.
os.makedirs(os.path.join(home, "chaveiro"), exist_ok=True)
json.dump({"claudeAiOauth": {"accessToken": "tok-principal", "refreshToken": "never-used",
                             "expiresAt": int((now + 5 * 3600) * 1000),
                             "subscriptionType": "max", "rateLimitTier": "default_claude_max_20x"}},
          open(os.path.join(home, "chaveiro", "Claude Code-credentials"), "w"))
def b64(o):
    return base64.urlsafe_b64encode(json.dumps(o).encode()).decode().rstrip("=")
claims = {"exp": int(now + 86400),
          "https://api.openai.com/profile": {"email": "jose@exemplo.com"},
          "https://api.openai.com/auth": {"chatgpt_plan_type": "pro", "chatgpt_account_id": "ACC"}}
access = "h." + b64(claims) + ".s"
json.dump({"auth_mode": "chatgpt", "OPENAI_API_KEY": None,
           "tokens": {"access_token": access, "account_id": "ACC", "refresh_token": "never-used"}},
          open(os.path.join(home, ".codex/auth.json"), "w"))
open(os.path.join(home, "codex-token"), "w").write(access)
# The Jev's key, in the stand-in Keychain: the one thing that makes the Jev's
# credit show.
open(os.path.join(home, "chaveiro", "openrouter-api-key"), "w").write("sk-or-do-teste\n")
PY

# ----------------------------------------------- the stand-in for both services
cat >"$WORK/standin.py" <<'PY'
import json, os, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
log = open(sys.argv[2], "a", buffering=1)
# While this file is there every answer takes six seconds: a round of
# readings kept in flight on purpose.
slow = os.path.join(os.path.dirname(sys.argv[2]), "slow")
codex_token = open(sys.argv[3]).read()
reset = time.strftime("%Y-%m-%dT%H:%M:%S.123456+00:00", time.gmtime(time.time() + 4 * 3600 + 600))
week = time.strftime("%Y-%m-%dT%H:%M:%S.654321+00:00", time.gmtime(time.time() + 37 * 3600))
CLAUDE = {"five_hour": {"utilization": 17.0, "resets_at": reset},
          "seven_day": {"utilization": 88.0, "resets_at": week},
          "seven_day_opus": None, "seven_day_sonnet": None,
          "extra_usage": {"is_enabled": False, "utilization": None},
          "limits": [{"kind": "weekly_scoped", "percent": 32, "resets_at": week,
                      "scope": {"model": {"display_name": "Fable"}}}]}
CODEX = {"plan_type": "pro", "rate_limit": {"limit_reached": False,
         "primary_window": {"used_percent": 2, "limit_window_seconds": 604800,
                            "reset_at": int(time.time() + 6 * 86400)},
         "secondary_window": None}}
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        if os.path.exists(slow):
            time.sleep(6)
        auth = self.headers.get("Authorization", "")
        token = auth[len("Bearer "):] if auth.startswith("Bearer ") else ""
        who = "codex" if token == codex_token else token
        log.write(json.dumps({"path": self.path, "token": who, "ua": self.headers.get("User-Agent", ""),
                              "account": self.headers.get("ChatGPT-Account-Id")}) + "\n")
        if self.path == "/profile" and token == "tok-principal":
            status, body = 200, {"account": {"uuid": "U1", "email_address": "um@exemplo.com"}}
        elif self.path == "/claude" and token == "tok-principal":
            status, body = 200, CLAUDE
        elif self.path == "/claude" and token == "tok-429":
            status, body = 429, {"error": "rate_limited"}
        elif self.path == "/codex" and token == codex_token:
            status, body = 200, CODEX
        elif self.path == "/or/credits" and token == "sk-or-do-teste":
            status, body = 200, {"data": {"total_credits": 10, "total_usage": 0.0246}}
        elif self.path == "/or/key" and token == "sk-or-do-teste":
            # Without a ceiling once this file is there: the key spends from the whole credit.
            limit = None if os.path.exists(os.path.join(os.path.dirname(sys.argv[2]), "jev-sem-teto")) else 5
            status, body = 200, {"data": {"limit": limit, "usage": 0.52, "usage_daily": 0.0018, "usage_monthly": 0.52}}
        else:
            status, body = 401, {"error": "unknown token"}
        data = json.dumps(body).encode()
        self.send_response(status)
        if status == 429: self.send_header("Retry-After", "120")
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
server = ThreadingHTTPServer(("127.0.0.1", 0), H)
open(sys.argv[1], "w").write(str(server.server_address[1]))
server.serve_forever()
PY
python3 "$WORK/standin.py" "$WORK/port" "$WORK/requests.jsonl" "$FAKE/codex-token" &
SERVER_PID=$!
waited=0
while [ ! -s "$WORK/port" ] && [ "$waited" -lt 40 ]; do sleep 0.25; waited=$((waited + 1)); done
PORT=$(cat "$WORK/port")

export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$WORK/state
export KEEP_AI_USAGE_HOME=$FAKE
export KEEP_AI_USAGE_CLAUDE_URL=http://127.0.0.1:$PORT/claude
export KEEP_AI_USAGE_CODEX_URL=http://127.0.0.1:$PORT/codex
export KEEP_IA_PERFIL_URL=http://127.0.0.1:$PORT/profile
export KEEP_IA_OPENROUTER_URL=http://127.0.0.1:$PORT/or
# The Keychain the core reads Claude Code's own login from: `security` as the
# core calls it, answering out of the made-up home.
cat >"$WORK/security" <<'SH'
#!/bin/sh
S=
while [ $# -gt 0 ]; do [ "$1" = -s ] && { shift; S="$1"; }; shift; done
F="$KEEP_AI_USAGE_HOME/chaveiro/$S"
[ -f "$F" ] || exit 44
cat "$F"
SH
chmod +x "$WORK/security"
export KEEP_IA_SECURITY=$WORK/security
# The core, the real one in the bundle, reached through a stand-in that logs
# what it is asked and plays it where the test needs it slow, refusing or
# opening a login; its logins' tabs open in this daemon.
mkdir -p "$WORK/ia"
cp tools/ia-test/fake-keep-ia.py "$WORK/fake-keep-ia.py"
chmod +x "$WORK/fake-keep-ia.py"
export KEEP_IA_BIN=$WORK/fake-keep-ia.py
export FAKE_IA_DIR=$WORK/ia
export FAKE_IA_KEEP=$KEEP
export FAKE_IA_CORE=$KEEP
mkdir -p "$KEEP_STATE_DIR"
# Unfolded, as the checks below read it: a run cut short while it was folded
# would otherwise leave the next one reading the other form.
defaults write dev.luhw.keep.dev usageFooterFolded -bool false 2>/dev/null
require_scratch_socket
rm -f "$SOCKET"

"$APP/Contents/Resources/keepd" >>"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
waited=0
while [ ! -S "$SOCKET" ] && [ "$waited" -lt 20 ]; do sleep 0.25; waited=$((waited + 1)); done

stop_app
: >"$WORK/app.log"
KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
APP_PID=$!
waited=0
while ! grep -aqE "usage +uso devidas: " "$WORK/app.log" && [ "$waited" -lt 120 ]; do
    sleep 0.25; waited=$((waited + 1))
done
place_on_screen
sleep 2
"$AXTEXT" "$APP_NAME" >"$WORK/ax.txt" 2>/dev/null

say ""
say "every account, one line per login"
check "the footer is there" yes "$(has "Consumo de IA")"
check "the active account, by the name the tabs know it by" yes "$(has "Claude · principal")"
check "its duplicate slot and its global login are not more accounts" no "$(has "Claude · assinaturas")"
check "the account whose access lapsed" yes "$(has "Claude · reserva")"
check "the account the service is holding back" yes "$(has "Claude · limitada")"
check "the Codex login, its only one: principal" yes "$(has "GPT · principal")"
check "each account's address is written out" yes "$(has "um@exemplo.com")"
check "the lapsed account's address too" yes "$(has "dois@exemplo.com")"
check "and the Codex one" yes "$(has "jose@exemplo.com")"

say ""
say "the bars carry what the services answered"
check "Claude's five-hour window" yes "$(has "Claude · principal 5h 17% reinicia em 4h10")"
check "Claude's weekly window" yes "$(has "Claude · principal 7d 88% reinicia em 1d13h")"
check "Claude's weekly window for one model" yes "$(has "Claude · principal Fable 32%")"
check "Codex's weekly window" yes "$(has "GPT · principal 7d 2% reinicia em 6d")"
check "a lapsed access says so instead of a figure" yes "$(has "acesso vencido")"
check "a 'too many requests' says so" yes "$(has "consultas demais (HTTP 429)")"
check "and neither shows a made-up bar" no "$(has "Claude · reserva 5h")"

say ""
say "what was asked, and how"
asked() { python3 -c 'import json,sys; print(" ".join(sorted({r["token"] for r in map(json.loads, open(sys.argv[1])) if not r["path"].startswith("/or/")})))' "$WORK/requests.jsonl"; }
check "each live login was asked once per round, the lapsed one never" "codex tok-429 tok-principal" "$(asked)"
check "with the fresher token of the duplicate pair" no "$(grep -qF tok-principal-velho "$WORK/requests.jsonl" && echo yes || echo no)"
check "Claude asked with a CLI's user agent" yes "$(grep -F '"token": "tok-principal"' "$WORK/requests.jsonl" | grep -qE '"ua": "claude-cli/[0-9.]+ \(external, cli\)"' && echo yes || echo no)"
check "Codex asked for its workspace" yes "$(grep -F '"token": "codex"' "$WORK/requests.jsonl" | grep -qF '"account": "ACC"' && echo yes || echo no)"
check "no token in the trace" no "$(grep -qE 'tok-|Bearer|sk-or' "$WORK/app.log" && echo yes || echo no)"

say ""
say "the Jev's credit"
check "the Jev has a block of its own" yes "$(has "Jev · OpenRouter")"
check "the account's credit left: ten bought, a little spent" yes "$(has "Jev conta: restam US\$ 9,98")"
check "the key's ceiling left: five, half a dollar spent" yes "$(has "Jev chave: restam US\$ 4,48")"
check "the credit was asked with the key from the Keychain, on both routes" "/or/credits /or/key" \
    "$(python3 -c 'import json,sys; print(" ".join(sorted({r["path"] for r in map(json.loads, open(sys.argv[1])) if r["token"] == "sk-or-do-teste"})))' "$WORK/requests.jsonl")"
check "and the key went to no other service" no \
    "$(python3 -c 'import json,sys; print("yes" if any(r["token"] == "sk-or-do-teste" and not r["path"].startswith("/or/") for r in map(json.loads, open(sys.argv[1]))) else "no")' "$WORK/requests.jsonl")"

say ""
say "the order of priority, with the kit's helper"
line_of() { grep -nF -- "$1" "$WORK/ax.txt" | head -1 | cut -d: -f1; }
first_of() {  # first_of <a> <b>: whichever of the two lines comes first
    local a b
    a=$(line_of "$1"); b=$(line_of "$2")
    if [ -n "$a" ] && { [ -z "$b" ] || [ "$a" -lt "$b" ]; }; then echo "$1"
    elif [ -n "$b" ]; then echo "$2"
    else echo none; fi
}
ax() { "$AXTEXT" "$APP_NAME" >"$WORK/ax.txt" 2>/dev/null; }
until_text() {  # until_text <text> <tenths>
    local tries=$2
    while [ "$tries" -gt 0 ]; do
        ax
        [ "$(has "$1")" = yes ] && { echo yes; return; }
        sleep 0.1
        tries=$((tries - 1))
    done
    echo no
}
# The same, but looked for only until a moment on the clock ($SECONDS): for
# what must happen before something else would do it anyway.
until_text_by() {  # until_text_by <text> <deadline, in $SECONDS>
    while [ "$SECONDS" -lt "$2" ]; do
        ax
        [ "$(has "$1")" = yes ] && { echo yes; return; }
        sleep 0.1
    done
    echo no
}
calls() { python3 -c 'import json,sys
try:
    for l in open(sys.argv[1]): print(" ".join(json.loads(l)["argv"]))
except FileNotFoundError: pass' "$WORK/ia/calls.jsonl"; }
ordem() { tr '\n' ' ' <"$FAKE/.claude/contas/.ordem" 2>/dev/null | sed 's/ $//'; }
ax
check "each account carries its place in the order" yes "$(has "1 · Claude · principal")"
check "the rest where they always were: the one the tabs run on, then by name, Claude before GPT" "yes yes yes" \
    "$(has "2 · Claude · limitada") $(has "3 · Claude · reserva") $(has "4 · GPT · principal")"
check "the first can go down" yes "$(has "Descer Claude · principal")"
check "but not up" no "$(has "Subir Claude · principal")"
check "the last can go up" yes "$(has "Subir GPT · principal")"
check "but not down" no "$(has "Descer GPT · principal")"
check "and there is a way to sign in to another account" yes "$(has "Entrar em outra conta")"

# The kit takes five seconds to answer: the screen does not wait for it.
echo 5 >"$WORK/ia/lento"
"$AXPRESS" "$APP_NAME" press "Descer Claude · principal"
sleep 0.5
ax
check "down: the order changes on screen at once, the kit still at it" \
    "1 · Claude · limitada" "$(first_of "1 · Claude · limitada" "1 · Claude · principal")"
check "and the one moved says its new place" yes "$(has "2 · Claude · principal")"
check "the core is asked to move it, by its key" "ordem mover claude:principal baixo --json" "$(calls | tail -1)"
check "as the app's keep: ia first" yes \
    "$(grep -qF '"grupo": "ia"' "$WORK/ia/calls.jsonl" && echo yes || echo no)"
check "and told which daemon the app is on" yes \
    "$(grep -qF "\"socket\": \"$SOCKET\"" "$WORK/ia/calls.jsonl" && echo yes || echo no)"
check "and which home it reads" yes \
    "$(grep -qF "\"casa\": \"$FAKE\"" "$WORK/ia/calls.jsonl" && echo yes || echo no)"
# Meanwhile a file the footer watches changes — the account in use written
# again — and the accounts are read afresh, the old order with them.
echo principal >"$VAULT/.ativa"
sleep 3
ax
check "a read of the files before the kit wrote does not put the old order back" \
    "1 · Claude · limitada" "$(first_of "1 · Claude · limitada" "1 · Claude · principal")"
sleep 2.5
check "the kit wrote the order the screen showed" \
    "claude:limitada claude:principal claude:reserva gpt:principal" "$(ordem)"
# Past the ten seconds the order is held against the files: what is shown then
# is what the files say.
sleep 5
ax
check "and after the hold the screen reads it from the file the same" \
    "1 · Claude · limitada" "$(first_of "1 · Claude · limitada" "1 · Claude · principal")"

# Now the kit says no, two seconds later.
echo 2 >"$WORK/ia/lento"
touch "$WORK/ia/falha"
"$AXPRESS" "$APP_NAME" press "Subir Claude · principal"
pressed=$SECONDS
sleep 0.5
ax
check "up, and the kit will refuse: shown at once all the same" \
    "1 · Claude · principal" "$(first_of "1 · Claude · limitada" "1 · Claude · principal")"
# Taken back when the refusal lands, two seconds after the press — not when
# the hold on the order shown runs out, ten seconds after it.
check "refused: taken back" yes "$(until_text_by "1 · Claude · limitada" $((pressed + 7)))"
check "and said, in the core's words" yes \
    "$(until_text "Não deu para mudar a ordem: falha simulada do keep ia" 20)"
check "the order the kit has is untouched" \
    "claude:limitada claude:principal claude:reserva gpt:principal" "$(ordem)"
rm -f "$WORK/ia/falha" "$WORK/ia/lento"

say ""
say "signing in to another account"
# A round of readings set out now, and kept in flight past the login: when it
# lands it must not take away an account it had not heard of.
touch "$WORK/slow"
"$AXPRESS" "$APP_NAME" press "Medir agora"
sleep 0.5
"$AXPRESS" "$APP_NAME" open "Entrar em outra conta"
check "+ offers a login to either service, and the Jev's connection apart" \
    "$(printf '\t1\tEntrar em outra conta do Claude…\n\t1\tEntrar em outra conta do GPT…\n---\n\t1\tReconectar o Jev ao OpenRouter…')" \
    "$("$AXPRESS" "$APP_NAME" items)"
"$AXPRESS" "$APP_NAME" pick "Entrar em outra conta do GPT…"
sleep 1
check "the core is asked for a GPT login in this window's workspace" \
    "entrar gpt --ws=$HOME_WS --json" "$(calls | tail -1)"
check "the login's account shows up within five seconds" yes "$(until_text "GPT · nova" 50)"
check "at the end of the order" yes "$(has "5 · GPT · nova")"
# The round lands when its slowest answer does: six seconds on (the limited
# account is not asked again before its Retry-After).
waited=0
while ! grep -aqE "usage +uso agora: [0-9]{4,}ms" "$WORK/app.log" && [ "$waited" -lt 60 ]; do
    sleep 0.25; waited=$((waited + 1))
done
rm -f "$WORK/slow"
sleep 1
ax
check "a round of readings that set out before it landed without taking it away" yes "$(has "GPT · nova")"
login_tab=$(grep -aoE "login in $HOME_WS/[0-9]+" "$WORK/app.log" | tail -1 | sed 's#.*/##')
check "and the tab the login is in is the one shown" yes \
    "$([ -n "$login_tab" ] && grep -aqE "switch +→ $HOME_WS/$login_tab " "$WORK/app.log" && echo yes || echo no)"

say ""
say "folded"
"$AXPRESS" "$APP_NAME" press "Consumo de IA"
sleep 1
ax
check "one line an account, and the arrows still beside each" "yes yes yes" \
    "$(has "Subir GPT · nova") $(has "Descer Claude · limitada") $(has "Subir Claude · reserva")"
check "folded for real: no bars" "no no" "$(has "Claude · reserva 5h") $(has "Claude · principal 5h")"
check "the Jev on one line, with what it can still spend: the key's, the smaller" yes "$(has "Jev OpenRouter restam US\$ 4,48")"
check "folded for real: no bar of the Jev's" no "$(has "Jev conta: restam")"
"$AXPRESS" "$APP_NAME" press "Consumo de IA"
sleep 1
# The refusal's word is said for a few seconds and then goes, and the footer
# is measured below without it.
gone=no
for _ in $(seq 1 150); do
    ax
    [ "$(has "Não deu para mudar a ordem")" = no ] && { gone=yes; break; }
    sleep 0.1
done
check "the refusal's word goes away by itself" yes "$gone"

say ""
say "under the list, and on the floor"
order=$(awk -v ws="$HOME_WS" '$0 ~ "^" ws && !w { w = NR } /^Consumo de IA/ && !f { f = NR } END { print (w && f && w < f) ? "yes" : "no" }' "$WORK/ax.txt")
check "the footer comes after the workspaces" yes "$order"
read -r before bottom <<<"$("$AXPOS" "$APP_NAME" "Consumo de IA")"
for i in $(seq 1 30); do "$KEEP" new "Espaco$i" >/dev/null 2>&1; done
sleep 4
# The app can still be busy with thirty new rows, and a question put to it
# meanwhile goes unanswered: asked again for a few seconds.
after=""
for _ in 1 2 3 4 5; do
    read -r after bottom_after <<<"$("$AXPOS" "$APP_NAME" "Consumo de IA")"
    [ -n "$after" ] && break
    sleep 1
done
check "thirty more workspaces do not move it" "$before" "$after"
check "and it is still in the window" yes "$([ "$after" -gt 0 ] && [ "$after" -lt "$bottom_after" ] && echo yes || echo no)"

say ""
say "the Jev's key without a ceiling"
touch "$WORK/jev-sem-teto"
sleep 15
"$AXPRESS" "$APP_NAME" press "Medir agora"
until_text "Jev gasto: gastou US\$ 0,52" 150 >/dev/null
check "the second counter is what the key spent" yes "$(has "Jev gasto: gastou US\$ 0,52")"
check "and the account's credit is still the first" yes "$(has "Jev conta: restam US\$ 9,98")"
check "no ceiling counter any more" no "$(has "Jev chave:")"
rm -f "$WORK/jev-sem-teto"

say ""
say "connecting the Jev from the +"
"$AXPRESS" "$APP_NAME" open "Entrar em outra conta"
"$AXPRESS" "$APP_NAME" pick "Reconectar o Jev ao OpenRouter…"
sleep 1
check "the core is asked for the OpenRouter login in this window's workspace" \
    "entrar openrouter --ws=$HOME_WS --json" "$(calls | tail -1)"

say ""
say "the Jev's key taken away"
rm -f "$FAKE/chaveiro/openrouter-api-key"
sleep 15
"$AXPRESS" "$APP_NAME" press "Medir agora"
gone=no
for _ in $(seq 1 150); do
    ax
    [ "$(has "Jev · OpenRouter")" = no ] && { gone=yes; break; }
    sleep 0.1
done
check "without a key there is no Jev, and the line goes" yes "$gone"
check "the accounts stay" yes "$(has "Claude · principal")"

say ""
say "without a keep to ask, no footer"
stop_app
: >"$WORK/app-without-helper.log"
KEEP_IA_BIN=/var/empty KEEP_TRACE=1 "$BIN" >"$WORK/app-without-helper.log" 2>&1 &
APP_PID=$!
waited=0
while ! grep -aqE "sidebar +order" "$WORK/app-without-helper.log" && [ "$waited" -lt 120 ]; do
    sleep 0.25; waited=$((waited + 1))
done
place_on_screen
sleep 4
ax
check "no accounts: the app reads no login itself" no "$(has "Claude · principal")"
check "no arrows" no "$(has "Subir Claude")"
check "no way to sign in" no "$(has "Entrar em outra conta")"
check "and no footer at all" no "$(has "Consumo de IA")"
check "the core was never asked" no \
    "$(grep -aqE "usage +uso " "$WORK/app-without-helper.log" && echo yes || echo no)"

say ""
say "$PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
