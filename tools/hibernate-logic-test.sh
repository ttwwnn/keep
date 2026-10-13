#!/usr/bin/env bash
#
# Which hidden tabs are hibernated (apps/macos/Sources/Keep/Model/HibernationPolicy.swift):
# the tab on screen is never chosen and counts against the limit, the rest
# keep their place by how recently they were shown, and a tab holding a
# selection, a composition or a place scrolled back to is not let go of; and
# the bytes a `text:` action types into a hibernated tab through the daemon.
# Built from that file alone: no app. Seconds.
#
#   tools/hibernate-logic-test.sh             the check
#   tools/hibernate-logic-test.sh --sabotage  and then four ways of getting it
#                                             wrong, each of which it must fail
set -uo pipefail
cd "$(dirname "$0")/.."
WORK=$(mktemp -d /tmp/keep-hibernate-logic-XXXXXX)
trap 'rm -rf "$WORK"' EXIT
SOURCE=apps/macos/Sources/Keep/Model/HibernationPolicy.swift

build() {  # build <HibernationPolicy.swift>
    swiftc -O "$1" tools/hibernate-logic-test/main.swift -o "$WORK/test" 2>"$WORK/build.log" || {
        grep -E "error" "$WORK/build.log" | head -5; return 1; }
}

build "$SOURCE" || exit 1
"$WORK/test"
status=$?
[ "${1:-}" = "--sabotage" ] || exit $status
[ $status -eq 0 ] || { echo "the check itself fails; nothing to sabotage"; exit 1; }

echo
echo "sabotage: the oldest are the ones kept"
sed 's/candidates\[\$0\].lastShown > candidates\[\$1\].lastShown/candidates[$0].lastShown < candidates[$1].lastShown/' "$SOURCE" > "$WORK/Sabotaged.swift"
cmp -s "$SOURCE" "$WORK/Sabotaged.swift" && { echo "the sabotage changed nothing"; exit 1; }
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: the tabs on screen do not count against the limit"
sed 's/let room = max(0, limit - visible)/let room = max(0, limit)/' "$SOURCE" > "$WORK/Sabotaged.swift"
cmp -s "$SOURCE" "$WORK/Sabotaged.swift" && { echo "the sabotage changed nothing"; exit 1; }
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: a blocked tab is let go of anyway"
sed 's/\.filter { !candidates\[\$0\].blocked }/.filter { _ in true }/' "$SOURCE" > "$WORK/Sabotaged.swift"
cmp -s "$SOURCE" "$WORK/Sabotaged.swift" && { echo "the sabotage changed nothing"; exit 1; }
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: a hex escape typed as its digits"
sed 's/guard rest.count >= 2, let byte = UInt8(rest.prefix(2), radix: 16) else { return nil }/guard rest.count >= 2 else { return nil }; let byte = UInt8(ascii: "x")/' "$SOURCE" > "$WORK/Sabotaged.swift"
cmp -s "$SOURCE" "$WORK/Sabotaged.swift" && { echo "the sabotage changed nothing"; exit 1; }
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi
