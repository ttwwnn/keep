#!/usr/bin/env bash
#
# The usage footer's facts (apps/macos/Sources/Keep/Model/AIUsage.swift):
# both services' answers in their real shape, the logins found in a made-up
# home, the stand-in address refused unless it is on this machine, and the
# strings the footer prints. And what the AI accounts rest on: the kit's
# order of priority and the extra GPT logins (AIUsage.swift), which account
# each tab is on and what its menu offers (Model/AIChoice.swift), and asking
# the kit's `keep-ia` (Daemon/KitHelper.swift) — against a stand-in that
# logs what it is asked. No app, no network, no real login, no real kit.
# Seconds.
#
#   tools/usage-test.sh              the check
#   tools/usage-test.sh --sabotage   and then each rule broken in turn, which
#                                    it must fail on
set -uo pipefail
cd "$(dirname "$0")/.."
WORK=$(mktemp -d /tmp/keep-usage-model-XXXXXX)
trap 'rm -rf "$WORK"' EXIT
USAGE=apps/macos/Sources/Keep/Model/AIUsage.swift
CHOICE=apps/macos/Sources/Keep/Model/AIChoice.swift
HELPER=apps/macos/Sources/Keep/Daemon/KitHelper.swift

build() {  # build <AIUsage.swift> <AIChoice.swift> <KitHelper.swift>
    swiftc -O "$1" "$2" "$3" tools/usage-test/main.swift -o "$WORK/test" 2>"$WORK/build.log" || {
        grep -E "error" "$WORK/build.log" | head -5; return 1; }
}
# The kit's helper, played by the stand-in the app's own tests use: it logs
# what it was asked, and answers out of the test's made-up home.
cp tools/ia-test/fake-keep-ia.py "$WORK/fake-keep-ia.py"
chmod +x "$WORK/fake-keep-ia.py"
run() { FALSO_IA=$WORK/fake-keep-ia.py FAKE_IA_DIR=$WORK "$WORK/test"; }

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
sabotage "$USAGE" "a merged account at the later of its places" \
    'if let first = listed.min(by: { $0.place < $1.place }) {|||if let first = listed.max(by: { $0.place < $1.place }) {' || ok=1
sabotage "$USAGE" "accounts the order does not name put first" \
    'return ordered + rest|||return rest + ordered' || ok=1
sabotage "$USAGE" "an extra GPT login that is the principal's shown twice" \
    'if let account = codex(auth: auth, alias: name) { found.append(account) }
        }
        return merge(found)|||if let account = codex(auth: auth, alias: name) { found.append(account) }
        }
        return found' || ok=1
sabotage "$USAGE" "an account at 95% still taking work" \
    '$0.percent >= 95 }|||$0.percent >= 96 }' || ok=1
sabotage "$USAGE" "the watcher blind to the order" \
    '[".ativa", ".preferida", ".ordem"]|||[".ativa", ".preferida"]' || ok=1
sabotage "$CHOICE" "following the order onto GPT sent as Claude's order" \
    'case .codex: return first.account.engine.key(first.account.alias)|||case .codex: return AIHelper.followOrder' || ok=1
sabotage "$CHOICE" "the dash on every Claude account, not the one in use" \
    'account.engine == .claude, account.isActive {|||account.engine == .claude {' || ok=1
sabotage "$CHOICE" "a tab running another program still offered choices" \
    'let enabled = program.allowsChoice|||let enabled = true' || ok=1
sabotage "$CHOICE" "Claude Code named by its version taken for another program" \
    'name == "claude" || Self.isVersionNumber(name)|||name == "claude"' || ok=1
sabotage "$CHOICE" "a retrato of another daemon believed within 10 ms" \
    'abs(born - start) < 0.001|||abs(born - start) < 0.01' || ok=1
sabotage "$CHOICE" "a retrato of another format read" \
    '(root["versao"] as? NSNumber)?.intValue == 1,|||(root["versao"] as? NSNumber) != nil,' || ok=1
sabotage "$CHOICE" "a switch made in the app outliving the kit's newer word" \
    'notes = notes.filter { $0.value.at > written }|||notes = notes.filter { _ in true }' || ok=1
sabotage "$CHOICE" "a tab back at its shell still said to be on an account" \
    'guard program.runsAI else { return nil }|||' || ok=1
sabotage "$CHOICE" "the sidebar naming Codex's own login" \
    'case "gpt" where alias != "principal": return "codex · \(alias)"|||case "gpt": return "codex · \(alias)"' || ok=1
sabotage "$HELPER" "KEEP_IA_BIN pointing at nothing falling through to the kit's" \
    'if let named = environment["KEEP_IA_BIN"] { return isRunnable(named) ? named : nil }|||if let named = environment["KEEP_IA_BIN"], isRunnable(named) { return named }' || ok=1
sabotage "$HELPER" "a directory taken for the helper" \
    '&& !directory.boolValue|||' || ok=1
sabotage "$HELPER" "a key with a colon in its name let through" \
    '#"^(claude|gpt):[^\s/:]+$"#|||#"^(claude|gpt):[^\s/]+$"#' || ok=1
sabotage "$HELPER" "a workspace passed as a word of its own" \
    'var arguments = ["trocar", "--ws=\(workspace)",|||var arguments = ["trocar", "--ws", workspace,' || ok=1
sabotage "$HELPER" "a key refused by nobody before the helper" \
    '        guard isValidKey(key) else { return .failure(invalid(key)) }
        var arguments|||        var arguments' || ok=1
sabotage "$HELPER" "no deadline" \
    'guard finished.wait(timeout: .now() + seconds) == .success else {|||guard finished.wait(timeout: .distantFuture) == .success else {' || ok=1
exit $ok
