#!/usr/bin/env bash
#
# The keys a cut sends to take selected text out of the line a program is
# editing (apps/macos/Sources/Keep/Model/CutSelection.swift). Built from that
# file alone: no app. Seconds.
#
#   tools/cut-test.sh             the check
#   tools/cut-test.sh --sabotage  and then the cursor always taken to be at the
#                                 selection's start, and an ambiguous match
#                                 taken as the first, each of which it must fail
set -uo pipefail
cd "$(dirname "$0")/.."
WORK=$(mktemp -d /tmp/keep-cut-XXXXXX)
trap 'rm -rf "$WORK"' EXIT
SOURCE=apps/macos/Sources/Keep/Model/CutSelection.swift

build() {  # build <CutSelection.swift>
    swiftc -O "$1" tools/cut-test/main.swift -o "$WORK/test" 2>"$WORK/build.log" || {
        grep -E "error" "$WORK/build.log" | head -5; return 1; }
}

build "$SOURCE" || exit 1
"$WORK/test"
status=$?
[ "${1:-}" = "--sabotage" ] || exit $status
[ $status -eq 0 ] || { echo "the check itself fails; nothing to sabotage"; exit 1; }

echo
echo "sabotage: the cursor is taken to be at the selection's start"
sed 's/let move = end - cursorColumn/let move = 0 * cursorColumn/' "$SOURCE" > "$WORK/Sabotaged.swift"
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: an ambiguous match is taken as the first"
sed 's/guard starts.count == 1, let start/guard starts.count >= 1, let start/' "$SOURCE" > "$WORK/Sabotaged.swift"
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: any selection empties the whole prompt"
sed 's/guard !squeezed(selected).isEmpty, squeezed(selected) == squeezed(all) else { return nil }/_ = all/' "$SOURCE" > "$WORK/Sabotaged.swift"
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: a word cut where the row ends is taken for two words"
sed 's/let cutMidWord = row.count >= width \&\& tail > 0 \&\& head > 0 \&\& tail + head > width/let cutMidWord = false/' "$SOURCE" > "$WORK/Sabotaged.swift"
grep -q 'let cutMidWord = false' "$WORK/Sabotaged.swift" || { echo "sabotage did not apply"; exit 1; }
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi
