#!/usr/bin/env bash
#
# What ⌘-click makes of a printed path (apps/macos/Sources/Keep/Model/
# LinkOpener.swift): absolute, relative to the shell's directory, `~`, a name
# with a space, a trailing :line or :line:col, a folder, a file that is not
# there, and a path pieced back from the screen when the runtime found only
# part of it (broken across rows, spaces escaped). Built from that file alone: no app. Seconds.
#
#   tools/link-test.sh             the check
#   tools/link-test.sh --sabotage  and then the line suffix left in, rows
#                                  never joined and escaped spaces ending
#                                  the word, each of which it must fail
set -uo pipefail
cd "$(dirname "$0")/.."
WORK=$(mktemp -d /tmp/keep-link-XXXXXX)
trap 'rm -rf "$WORK"' EXIT
SOURCE=apps/macos/Sources/Keep/Model/LinkOpener.swift

build() {  # build <LinkOpener.swift>
    swiftc -O "$1" tools/link-test/main.swift -o "$WORK/test" 2>"$WORK/build.log" || {
        grep -E "error" "$WORK/build.log" | head -5; return 1; }
}

build "$SOURCE" || exit 1
"$WORK/test"
status=$?
[ "${1:-}" = "--sabotage" ] || exit $status
[ $status -eq 0 ] || { echo "the check itself fails; nothing to sabotage"; exit 1; }

echo
echo "sabotage: the :line suffix is never taken off"
sed 's/for _ in 0..<2 {/for _ in 0..<0 {/' "$SOURCE" > "$WORK/Sabotaged.swift"
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: a path broken across rows is never joined"
sed 's/before.count < 3 {/before.count < 0 {/; s/after.count < 3 {/after.count < 0 {/' \
    "$SOURCE" > "$WORK/Sabotaged.swift"
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: an escaped space ends the word"
sed 's/|| (low >= 2 \&\& chars\[low - 2\] == "\\\\")//; s/|| (high > 0 \&\& chars\[high - 1\] == "\\\\")//' \
    "$SOURCE" > "$WORK/Sabotaged.swift"
cmp -s "$SOURCE" "$WORK/Sabotaged.swift" && { echo "the sabotage did not apply"; exit 1; }
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi
