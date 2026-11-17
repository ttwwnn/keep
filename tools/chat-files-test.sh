#!/usr/bin/env bash
#
# What the panel beside a conversation lists (apps/macos/Sources/Keep/Model/
# ChatFiles.swift), read from a transcript made here: the files the person sent
# (dropped, typed, a pasted picture) and the ones the conversation made
# (written, handed over, created by a command), and not what a summary, a task
# or another tab named. Built from that file alone: no app. Seconds.
#
#   tools/chat-files-test.sh             the check
#   tools/chat-files-test.sh --sabotage  and then a file only read counted as made,
#                                        the summary's files counted as sent, and a
#                                        step on the way counted as made
set -uo pipefail
cd "$(dirname "$0")/.."
WORK=$(mktemp -d /tmp/keep-chat-files-XXXXXX)
trap 'rm -rf "$WORK"' EXIT
SOURCE=apps/macos/Sources/Keep/Model/ChatFiles.swift

build() {  # build <ChatFiles.swift>
    swiftc -O "$1" tools/chat-files-test/main.swift -o "$WORK/test" 2>"$WORK/build.log" || {
        grep -E "error" "$WORK/build.log" | head -5; return 1; }
}

build "$SOURCE" || exit 1
"$WORK/test"
status=$?
[ "${1:-}" = "--sabotage" ] || exit $status
[ $status -eq 0 ] || { echo "the check itself fails; nothing to sabotage"; exit 1; }

echo
echo "sabotage: a file named by a command counts whenever it was made"
sed 's/return date >= begun/return date >= .distantPast/' "$SOURCE" > "$WORK/Sabotaged.swift"
cmp -s "$SOURCE" "$WORK/Sabotaged.swift" && { echo "the sabotage did not apply"; exit 1; }
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: the summary of a compaction is taken as the person's"
sed 's/guard json\["isCompactSummary"\] as? Bool != true,/guard true,/' "$SOURCE" > "$WORK/Sabotaged.swift"
cmp -s "$SOURCE" "$WORK/Sabotaged.swift" && { echo "the sabotage did not apply"; exit 1; }
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: a file it wrote counts though no answer shows it"
sed 's/return entry.handed || answered.contains/return true || answered.contains/' "$SOURCE" > "$WORK/Sabotaged.swift"
cmp -s "$SOURCE" "$WORK/Sabotaged.swift" && { echo "the sabotage did not apply"; exit 1; }
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi
