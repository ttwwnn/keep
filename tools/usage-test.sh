#!/usr/bin/env bash
#
# What the app holds of the AI accounts, and how it asks for them
# (apps/macos/Sources/Keep/Model/AIUsage.swift, Model/AIChoice.swift and
# Daemon/KeepCLI.swift): the core's usage answer read as the core writes it
# and kept as the kit reads it, the files watched for a login just made, the
# strings the footer prints, which account each tab is on and what its menu
# offers, and asking the `keep` inside the app — where it is, what it is told,
# and every question in the contract (docs/ia.md), against a stand-in that
# logs what it is asked. Then the same against the real core
# (crates/keep-ia, built here), in a home of its own, measuring a stand-in of
# the services on 127.0.0.1: what the Swift reads is what the Rust writes.
# No app, no real login, no real kit, no network. Seconds, plus a cargo build.
#
#   tools/usage-test.sh              the check
#   tools/usage-test.sh --sabotage   and then each rule broken in turn, which
#                                    it must fail on
#   SKIP_CORE=1 tools/usage-test.sh  without the real core
set -uo pipefail
cd "$(dirname "$0")/.."
WORK=$(mktemp -d /tmp/keep-usage-model-XXXXXX)
SERVER_PID=
cleanup() {
    [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null
    rm -rf "$WORK"
}
trap cleanup EXIT
USAGE=apps/macos/Sources/Keep/Model/AIUsage.swift
CHOICE=apps/macos/Sources/Keep/Model/AIChoice.swift
HELPER=apps/macos/Sources/Keep/Daemon/KeepCLI.swift

build() {  # build <AIUsage.swift> <AIChoice.swift> <KeepCLI.swift>
    swiftc -O "$1" "$2" "$3" tools/usage-test/main.swift -o "$WORK/test" 2>"$WORK/build.log" || {
        grep -E "error" "$WORK/build.log" | head -5; return 1; }
}
# The core, played by the stand-in the app's own tests use: it logs what it
# was asked, and answers out of the test's made-up home.
cp tools/ia-test/fake-keep-ia.py "$WORK/fake-keep-ia.py"
chmod +x "$WORK/fake-keep-ia.py"
mkdir -p "$WORK/home" "$WORK/ia"
# A Keychain that holds the Jev's key and nothing else, for the real core.
cat >"$WORK/security-com-chave" <<'SH'
#!/bin/sh
case "$*" in
    *"-s openrouter-api-key"*) echo "sk-or-do-teste"; exit 0 ;;
esac
exit 44
SH
chmod +x "$WORK/security-com-chave"

# The real core, in a home of its own: a slot of the kit's vault and Codex's
# login, measured by a stand-in of both services on this machine.
NUCLEO=
if [ -z "${SKIP_CORE:-}" ]; then
    if cargo build -p keep >"$WORK/cargo.log" 2>&1; then
        NUCLEO=$PWD/target/debug/keep
    else
        echo "could not build the core (cargo build -p keep); the tail of its log:"
        tail -5 "$WORK/cargo.log"
        exit 1
    fi
    CORE_HOME=$WORK/casa-nucleo
    mkdir -p "$CORE_HOME/.claude/contas" "$CORE_HOME/.codex"
    python3 - "$CORE_HOME" <<'PY'
import base64, json, os, sys, time
home = sys.argv[1]
now = time.time()
json.dump({"apelido": "principal", "email": "um@exemplo.com", "accountUuid": "U1",
           "credenciais": {"claudeAiOauth": {"accessToken": "tok-principal", "refreshToken": "never-used",
                                             "expiresAt": int((now + 5 * 3600) * 1000),
                                             "subscriptionType": "max", "rateLimitTier": "default_claude_max_20x"}}},
          open(os.path.join(home, ".claude/contas/principal.json"), "w"))
def b64(o):
    return base64.urlsafe_b64encode(json.dumps(o).encode()).decode().rstrip("=")
access = "h." + b64({"exp": int(now + 86400), "https://api.openai.com/profile": {"email": "jose@exemplo.com"},
                     "https://api.openai.com/auth": {"chatgpt_plan_type": "pro", "chatgpt_account_id": "ACC"}}) + ".s"
json.dump({"auth_mode": "chatgpt", "tokens": {"access_token": access, "account_id": "ACC"}},
          open(os.path.join(home, ".codex/auth.json"), "w"))
PY
    cat >"$WORK/standin.py" <<'PY'
import json, sys, time
from http.server import BaseHTTPRequestHandler, HTTPServer
reset = time.strftime("%Y-%m-%dT%H:%M:%S.123456+00:00", time.gmtime(1791100000))
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        token = self.headers.get("Authorization", "")[len("Bearer "):]
        if self.path == "/claude" and token == "tok-principal":
            status, body = 200, {"five_hour": {"utilization": 17.0, "resets_at": reset},
                                 "seven_day": {"utilization": 50, "resets_at": None}}
        elif self.path == "/codex" and self.headers.get("ChatGPT-Account-Id") == "ACC":
            status, body = 200, {"rate_limit": {"limit_reached": True, "primary_window": {
                "used_percent": 100, "limit_window_seconds": 604800, "reset_at": 1791200000}}}
        elif self.path == "/or/credits" and token == "sk-or-do-teste":
            status, body = 200, {"data": {"total_credits": 10, "total_usage": 0.0246}}
        elif self.path == "/or/key" and token == "sk-or-do-teste":
            status, body = 200, {"data": {"limit": 5, "usage": 0.0042, "usage_daily": 0.0018, "usage_monthly": 0.0042}}
        else:
            status, body = 401, {}
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
server = HTTPServer(("127.0.0.1", 0), H)
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
fi

run() {
    # The core reads its stand-ins under the names the app's tests have
    # always set; a measurement starts afresh each run.
    rm -rf "${CORE_HOME:-/nonexistent}/.keep-ia-estado"
    rm -f "${CORE_HOME:-/nonexistent}/.claude/contas/.ordem"
    KEEP_AI_USAGE_HOME=$WORK/home KEEP_IA_BIN=/var/empty \
        KEEP_AI_USAGE_CLAUDE_URL=http://127.0.0.1:${PORT:-9}/claude \
        KEEP_AI_USAGE_CODEX_URL=http://127.0.0.1:${PORT:-9}/codex \
        KEEP_IA_OPENROUTER_URL=http://127.0.0.1:${PORT:-9}/or \
        SEGURANCA_COM_CHAVE=$WORK/security-com-chave \
        NUCLEO=$NUCLEO CASA_NUCLEO=${CORE_HOME:-} \
        FALSO_IA=$WORK/fake-keep-ia.py FAKE_IA_DIR=$WORK/ia "$WORK/test"
}

build "$USAGE" "$CHOICE" "$HELPER" || exit 1
run
status=$?
[ "${1:-}" = "--sabotage" ] || exit $status
[ $status -eq 0 ] || { echo "the check itself fails; nothing to sabotage"; exit 1; }

echo
echo "sabotage: each of these must fail"
sabotage() {  # sabotage <file> <what> <old|||new>
    python3 - "$1" "$WORK/Broken.swift" "$3" <<'PY'
import sys
s = open(sys.argv[1]).read()
old, new = sys.argv[3].split("|||")
if s.count(old) != 1:
    sys.exit("sabotage target found %d times: %r" % (s.count(old), old))
open(sys.argv[2], "w").write(s.replace(old, new))
PY
    [ $? -eq 0 ] || { echo "  could not sabotage: $2"; return 1; }
    local usage=$USAGE choice=$CHOICE helper=$HELPER
    case "$1" in
        "$USAGE") usage=$WORK/Broken.swift ;;
        "$CHOICE") choice=$WORK/Broken.swift ;;
        "$HELPER") helper=$WORK/Broken.swift ;;
    esac
    build "$usage" "$choice" "$helper" || { echo "  broken code did not build: $2"; return 1; }
    if run >"$WORK/out.txt" 2>&1; then
        echo "  NOT CAUGHT  $2"; return 1
    fi
    echo "  caught      $2: $(grep -m1 FALHA "$WORK/out.txt" | cut -c7-)"
}
ok=0
sabotage "$USAGE" "an account at its limit still taking work" \
    '$0.percent >= 100 }|||$0.percent >= 101 }' || ok=1
sabotage "$USAGE" "an account near its limit (95%) taken for one at it" \
    '$0.percent >= 100 }|||$0.percent >= 95 }' || ok=1
sabotage "$USAGE" "the watcher blind to the order" \
    '[".ativa", ".preferida", ".ordem"]|||[".ativa", ".preferida"]' || ok=1
sabotage "$USAGE" "the watcher blind to Keep's own logins" \
    'for file in ["keep.json", "conta.json"] {|||for file in ["conta.json"] {' || ok=1
sabotage "$USAGE" "the core's dates read as Foundation's own" \
    '        decoder.dateDecodingStrategy = .secondsSince1970
        return try? decoder.decode(UsageAnswer.self, from: data)|||        return try? decoder.decode(UsageAnswer.self, from: data)' || ok=1
sabotage "$USAGE" "the smaller of the two credits not the one that counts" \
    'keyLeft.map { min($0, accountLeft) } ?? accountLeft|||keyLeft.map { max($0, accountLeft) } ?? accountLeft' || ok=1
sabotage "$USAGE" "a key with no ceiling measured against nothing" \
    'var keySharePercent: Double { total > 0 ? min(100, (keyUsed ?? 0) / total * 100) : 0 }|||var keySharePercent: Double { 0 }' || ok=1
sabotage "$USAGE" "the watcher blind to the Jev's reading" \
    'parts.append("jev " + stamp(home.appendingPathComponent(state + "/jev.json")))|||_ = state' || ok=1
sabotage "$USAGE" "a credit that rounds to nothing said as zero" \
    'if value > 0, value < 0.01 { return "< US$ 0,01" }|||' || ok=1
sabotage "$USAGE" "more than two places in the Jev's figures" \
    'String(format: "%.2f", value)|||String(format: "%.4f", value)' || ok=1
sabotage "$USAGE" "the watcher blind to the Jev's book of calls" \
    'parts.append("jev livro " + stamp(home.appendingPathComponent(".claude/jev/chamadas.jsonl")))|||' || ok=1
sabotage "$USAGE" "the book of calls not counted as the Jev's own" \
    'parts.append("jev livro "|||parts.append("livro "' || ok=1
sabotage "$USAGE" "the Jev's dates read as Foundation's own" \
    '        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .secondsSince1970
        return try? decoder.decode(JevAnswer.self, from: data)|||        return try? JSONDecoder().decode(JevAnswer.self, from: data)' || ok=1
sabotage "$HELPER" "the Jev asked by the wrong command" \
    'askRaw(["jev"] + arguments + ["--json"]|||askRaw(["uso"] + arguments + ["--json"]' || ok=1
sabotage "$HELPER" "a click on Medir agora not passed on to the Jev" \
    'case .now: arguments = ["--agora"]
        case .cached: arguments = ["--cache"]|||case .now, .cached: arguments = ["--cache"]' || ok=1
sabotage "$HELPER" "a test app sending a refresh token to the real token service" \
    '            ("KEEP_IA_TOKEN_URL", ["KEEP_IA_TOKEN_URL"]),
|||' || ok=1
sabotage "$HELPER" "a test app letting the Jev's key reach the real OpenRouter" \
    '            ("KEEP_IA_OPENROUTER_URL", ["KEEP_IA_OPENROUTER_URL"]),
|||' || ok=1
sabotage "$CHOICE" "following the order sent as the account first in it" \
    'kind: .follow, title: followTitle, key: AIHelper.followOrder,|||kind: .follow, title: followTitle, key: lines.first(where: \.isAvailable).map { $0.account.engine.key($0.account.alias) } ?? AIHelper.followOrder,' || ok=1
sabotage "$CHOICE" "the dash on every Claude account, not the one in use" \
    '} else if account.engine == .claude, account.isActive {|||} else if account.engine == .claude {' || ok=1
sabotage "$CHOICE" "the dash on the shared login even when the core said where the tab runs" \
    'if account.keys.contains(running) { mark = .mixed }|||if account.engine == .claude, account.isActive { mark = .mixed }' || ok=1
sabotage "$CHOICE" "a tab running another program still offered choices" \
    'let enabled = program.allowsChoice|||let enabled = true' || ok=1
sabotage "$CHOICE" "Claude Code named by its version taken for another program" \
    'name == "claude" || Self.isVersionNumber(name)|||name == "claude"' || ok=1
sabotage "$CHOICE" "an answer about another daemon believed within 2 ms" \
    'max(saidStart, mine) - min(saidStart, mine) <= 1|||max(saidStart, mine) - min(saidStart, mine) <= 2' || ok=1
sabotage "$CHOICE" "an old daemon's answer believed from another process" \
    'guard let mine = daemonPid, let saidPid, saidPid == mine else { return nil }|||guard daemonPid != nil else { return nil }' || ok=1
sabotage "$CHOICE" "an answer of another format read" \
    '(root["versao"] as? NSNumber)?.intValue == 1,|||(root["versao"] as? NSNumber) != nil,' || ok=1
sabotage "$CHOICE" "a switch made in the app outliving the core's newer word" \
    'notes = notes.filter { $0.value.at > askedAt }|||notes = notes.filter { _ in true }' || ok=1
sabotage "$CHOICE" "an older answer laid over a newer one" \
    'guard askedAt >= asked,|||guard true,' || ok=1
sabotage "$CHOICE" "a tab back at its shell still said to be on an account" \
    '    func account(workspace: String, tab: UInt32, program: AIProgramKind) -> String? {
        guard program.runsAI else { return nil }|||    func account(workspace: String, tab: UInt32, program: AIProgramKind) -> String? {' || ok=1
sabotage "$CHOICE" "the footer lighting the choice over the account the tab runs on" \
    'if let running = entry.running, running.hasPrefix(prefix) { return running }|||' || ok=1
sabotage "$CHOICE" "the sidebar naming Codex's own login" \
    'case "gpt" where alias != "principal": return "codex · \(alias)"|||case "gpt": return "codex · \(alias)"' || ok=1
sabotage "$HELPER" "KEEP_IA_BIN pointing at nothing falling through to the app's keep" \
    'if let named { return isRunnable(named) ? named : nil }|||if let named, isRunnable(named) { return named }' || ok=1
sabotage "$HELPER" "a directory taken for the keep" \
    '&& !directory.boolValue|||' || ok=1
sabotage "$HELPER" "the core not told which daemon" \
    'environment["KEEP_SOCKET"] = Daemon.socketPath|||' || ok=1
sabotage "$HELPER" "a test app handing the core the real home" \
    'environment["KEEP_IA_HOME"] = home|||' || ok=1
sabotage "$HELPER" "a test app letting the core at the real Keychain" \
    'if base["KEEP_IA_SECURITY"] == nil {|||if false {' || ok=1
sabotage "$HELPER" "a test app letting a token reach the real services" \
    'environment[name] = nowhere|||' || ok=1
sabotage "$HELPER" "a stand-in the test named by the older name overridden" \
    'where !names.contains(where: { base[$0] != nil })|||where base[name] == nil' || ok=1
sabotage "$HELPER" "the core called without its group" \
    'switch KeepCLI.run(helper, ["ia"] + arguments,|||switch KeepCLI.run(helper, arguments,' || ok=1
sabotage "$HELPER" "a key with a colon in its name let through" \
    '#"^(claude|gpt):[^\s/:]+$"#|||#"^(claude|gpt):[^\s/]+$"#' || ok=1
sabotage "$HELPER" "a workspace passed as a word of its own" \
    'var arguments = ["trocar", "--ws=\(workspace)",|||var arguments = ["trocar", "--ws", workspace,' || ok=1
sabotage "$HELPER" "a key refused by nobody before the core" \
    '        guard isValidKey(key) else { return .failure(invalid(key)) }
        var arguments|||        var arguments' || ok=1
sabotage "$HELPER" "no deadline" \
    'guard finished.wait(timeout: .now() + seconds) == .success else {|||guard finished.wait(timeout: .distantFuture) == .success else {' || ok=1
exit $ok
