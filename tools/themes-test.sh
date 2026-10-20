#!/usr/bin/env bash
#
# Keep's own theme on the Mac (apps/macos/Sources/Keep/Model/TerminalPrefs.swift,
# `KeepThemes`): "Keep Escuro" and "Keep Claro" written where the person's
# themes live when missing, never over a file of the same name, and the
# `dark:…,light:…` default only when both are there. Built from that file
# alone (tools/themes-test/main.swift), in a folder of its own. Seconds.
#
#   tools/themes-test.sh             the check
#   tools/themes-test.sh --sabotage  and then each rule broken, which it must
#                                    fail
set -uo pipefail
cd "$(dirname "$0")/.."
WORK=$(mktemp -d /tmp/keep-themes-XXXXXX)
trap 'rm -rf "$WORK"' EXIT
SOURCE=apps/macos/Sources/Keep/Model/TerminalPrefs.swift

build() {  # build <TerminalPrefs.swift>
    swiftc -O "$1" tools/themes-test/main.swift -o "$WORK/test" 2>"$WORK/build.log" || {
        grep -E "error" "$WORK/build.log" | head -5; return 1; }
}
run() {
    rm -rf "$WORK/run"; mkdir -p "$WORK/run"
    "$WORK/test" "$WORK/run"
}

build "$SOURCE" || exit 1
run
status=$?
[ "${1:-}" = "--sabotage" ] || exit $status
[ $status -eq 0 ] || { echo "the check itself fails; nothing to sabotage"; exit 1; }

echo
echo "sabotage: each of these must fail"
sabotage() {  # sabotage <what> <old|||new>
    python3 - "$SOURCE" "$WORK/Broken.swift" "$2" <<'PY'
import sys
s = open(sys.argv[1]).read()
old, new = sys.argv[3].split("|||")
if s.count(old) != 1:
    sys.exit("sabotage target found %d times: %r" % (s.count(old), old))
open(sys.argv[2], "w").write(s.replace(old, new))
PY
    [ $? -eq 0 ] || { echo "  could not sabotage: $1"; return 1; }
    build "$WORK/Broken.swift" || { echo "  broken code did not build: $1"; return 1; }
    rm -rf "$WORK/run"; mkdir -p "$WORK/run"
    if "$WORK/test" "$WORK/run" >"$WORK/out.txt"; then
        echo "  NOT CAUGHT  $1"; return 1
    fi
    echo "  caught      $1: $(grep -m1 FAIL "$WORK/out.txt" | cut -c9-)"
}
ok=0
sabotage "a file of the same name written over" \
    'guard !fm.fileExists(atPath: path) else { continue }|||' || ok=1
sabotage "the default without both halves" \
    'return found ? "theme = \(pair)" : nil|||return "theme = \(pair)"' || ok=1
sabotage "the halves swapped" \
    'static let pair = "dark:Keep Escuro,light:Keep Claro"|||static let pair = "dark:Keep Claro,light:Keep Escuro"' || ok=1
sabotage "the light ground lost" \
    'background = #ffffff|||background = #fafafa' || ok=1
exit $ok
