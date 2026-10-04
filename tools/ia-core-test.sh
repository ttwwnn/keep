#!/usr/bin/env bash
#
# A tab's AI, from the app's side, against the real core: the app's own
# Daemon.swift, KeepCLI.swift, AIUsage.swift and AIChoice.swift
# (tools/ia-core-test/main.swift) asking the `keep` built here — the one the
# app carries inside it — about a daemon of the test's own, whose tab runs a
# fake Claude Code and a fake Codex (crates/keep-ia/examples/ia_falsa.rs):
# following the order from a bare shell, a switch between Claude accounts and
# one to Codex and back, a busy tab refused and then interrupted, usage
# measured from a stand-in of the services, the order followed by itself, the
# menu built from all of it, a login opened in a tab, and the order moved.
# What the core prints is read exactly as the app reads it.
#
#   tools/ia-core-test.sh
#
# A made-up home (logins in a stand-in Keychain), stand-ins of the services on
# 127.0.0.1, a keepd of its own on a scratch socket. No window: it runs with
# the screen locked. About a minute, plus the cargo builds.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

WORK=$(mktemp -d /tmp/keep-ia-core-XXXXXX)
SERVER_PID=
DAEMON_PID=
cleanup() {
    [ -n "$DAEMON_PID" ] && kill "$DAEMON_PID" 2>/dev/null
    [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null
    [ -n "${KEEP_WORK:-}" ] && echo "kept: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT INT TERM

cargo build -p keep -p keepd >"$WORK/cargo.log" 2>&1 \
    && cargo build -p keep-ia --examples >>"$WORK/cargo.log" 2>&1 \
    || { echo "could not build keep, keepd or the fake AI; the tail of the log:"; tail -5 "$WORK/cargo.log"; exit 1; }
mkdir -p "$WORK/bin"
cp target/debug/examples/ia_falsa "$WORK/bin/claude"
cp target/debug/examples/ia_falsa "$WORK/bin/codex"
swiftc -O apps/macos/Sources/Keep/Daemon/Daemon.swift apps/macos/Sources/Keep/Daemon/KeepCLI.swift \
    apps/macos/Sources/Keep/Model/AIUsage.swift apps/macos/Sources/Keep/Model/AIChoice.swift \
    tools/ia-core-test/main.swift -o "$WORK/test" 2>"$WORK/build.log" \
    || { grep -E "error" "$WORK/build.log" | head -5; exit 1; }

# ------------------------------------------------------------ made-up logins
# Claude Code's own login (ana), a login of Keep's own for bia, and Codex's.
HOME_FAKE=$WORK/home
python3 - "$HOME_FAKE" <<'PY'
import base64, hashlib, json, os, sys, time, unicodedata
home = sys.argv[1]
now = time.time()
fixa = os.path.join(home, ".claude/contas/fixas/k-2")
os.makedirs(fixa)
os.makedirs(os.path.join(home, "chaveiro"))
os.makedirs(os.path.join(home, ".codex"))
def login(token):
    return json.dumps({"claudeAiOauth": {"accessToken": token, "refreshToken": "r",
                                         "expiresAt": int((now + 3600) * 1000),
                                         "rateLimitTier": "default_claude_max_20x", "subscriptionType": "max"}})
open(os.path.join(home, "chaveiro", "Claude Code-credentials"), "w").write(login("G"))
service = "Claude Code-credentials-" + hashlib.sha256(unicodedata.normalize("NFC", fixa).encode()).hexdigest()[:8]
open(os.path.join(home, "chaveiro", service), "w").write(login("F"))
json.dump({"versao": 1, "apelido": "bia", "email": "bia@y.com", "uuid": "u-2", "criadaEm": 1},
          open(os.path.join(fixa, "keep.json"), "w"))
def b64(o):
    return base64.urlsafe_b64encode(json.dumps(o).encode()).decode().rstrip("=")
access = b64({}) + "." + b64({"exp": int(now + 3600), "https://api.openai.com/profile": {"email": "ana@x.com"},
                              "https://api.openai.com/auth": {"chatgpt_account_id": "acct-1",
                                                              "chatgpt_plan_type": "pro"}}) + ".x"
json.dump({"tokens": {"access_token": access, "account_id": "acct-1"}},
          open(os.path.join(home, ".codex/auth.json"), "w"))
PY
cat >"$WORK/security" <<'SH'
#!/bin/sh
S=
while [ $# -gt 0 ]; do [ "$1" = -s ] && { shift; S="$1"; }; shift; done
F="$KEEP_AI_USAGE_HOME/chaveiro/$S"
[ -f "$F" ] || exit 44
cat "$F"
SH
chmod +x "$WORK/security"

# ----------------------------------------------- the stand-in for the services
cat >"$WORK/standin.py" <<'PY'
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
OWNERS = {"G": ("u-1", "ana@x.com"), "F": ("u-2", "bia@y.com")}
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        token = self.headers.get("Authorization", "")[len("Bearer "):]
        if self.path == "/profile" and token in OWNERS:
            uuid, email = OWNERS[token]
            status, body = 200, {"account": {"uuid": uuid, "email_address": email}}
        elif self.path == "/claude" and token == "G":
            status, body = 200, {"five_hour": {"utilization": 10}, "seven_day": {"utilization": 100}}
        elif self.path == "/claude" and token == "F":
            status, body = 200, {"five_hour": {"utilization": 1}, "seven_day": {"utilization": 2}}
        elif self.path == "/codex":
            status, body = 200, {"rate_limit": {"primary_window": {"used_percent": 3, "limit_window_seconds": 604800}}}
        else:
            status, body = 401, {}
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
server = ThreadingHTTPServer(("127.0.0.1", 0), H)
open(sys.argv[1], "w").write(str(server.server_address[1]))
server.serve_forever()
PY
python3 "$WORK/standin.py" "$WORK/port" &
SERVER_PID=$!
# A cold Python on a busy CI machine takes its time to listen.
waited=0
while [ ! -s "$WORK/port" ] && [ "$waited" -lt 240 ]; do sleep 0.25; waited=$((waited + 1)); done
PORT=$(cat "$WORK/port" 2>/dev/null)
[ -n "$PORT" ] || { echo "the stand-in of the services did not start"; exit 1; }

# ------------------------------------------------------- a daemon of its own
# Its tabs run /bin/sh with a plain prompt, and the fake AI they start keeps
# its sessions in the made-up home, as the real ones keep theirs in ~.
export KEEP_SOCKET=$WORK/k.sock
require_scratch_socket
export KEEP_AI_USAGE_HOME=$HOME_FAKE
export KEEP_IA_SECURITY=$WORK/security
export KEEP_IA_PERFIL_URL=http://127.0.0.1:$PORT/profile
export KEEP_IA_CLAUDE_URL=http://127.0.0.1:$PORT/claude
export KEEP_IA_CODEX_URL=http://127.0.0.1:$PORT/codex
export KEEP_IA_CLAUDE_BIN=$WORK/bin/claude
export KEEP_IA_CODEX_BIN=$WORK/bin/codex
export KEEP_IA_BIN=$PWD/target/debug/keep
KEEP_IA_HOME=$HOME_FAKE SHELL=/bin/sh PS1='$ ' target/debug/keepd >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
waited=0
while [ ! -S "$KEEP_SOCKET" ] && [ "$waited" -lt 120 ]; do sleep 0.25; waited=$((waited + 1)); done
[ -S "$KEEP_SOCKET" ] || { echo "the test's keepd did not start; its log:"; tail -5 "$WORK/daemon.log"; exit 1; }

KEEPD_PID=$DAEMON_PID DIGITA=$PWD/tools/ia-core-test/digita.py "$WORK/test"
