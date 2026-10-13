#!/usr/bin/env bash
#
# What the bar under a Claude Code conversation reads (apps/macos/Sources/Keep/
# Model/ClaudeStatus.swift): the status line's JSON, the agents listed under
# the prompt, and whether the prompt is empty. Built from that file alone: no
# app. Seconds.
#
#   tools/status-test.sh             the check
#   tools/status-test.sh --sabotage  and then agents counted above the prompt,
#                                    and a draft taken for an empty prompt,
#                                    each of which it must fail
set -uo pipefail
cd "$(dirname "$0")/.."
WORK=$(mktemp -d /tmp/keep-status-XXXXXX)
trap 'rm -rf "$WORK"' EXIT
SOURCE=apps/macos/Sources/Keep/Model/ClaudeStatus.swift

build() {  # build <ClaudeStatus.swift>
    swiftc -O "$1" tools/status-test/main.swift -o "$WORK/test" 2>"$WORK/build.log" || {
        grep -E "error" "$WORK/build.log" | head -5; return 1; }
}

build "$SOURCE" || exit 1
"$WORK/test"
status=$?
[ "${1:-}" = "--sabotage" ] || exit $status
[ $status -eq 0 ] || { echo "the check itself fails; nothing to sabotage"; exit 1; }

echo
echo "sabotage: agents are counted from the whole screen"
sed 's/guard let rule = lines.lastIndex(where: isRule) else { return 0 }/let rule = -1/' "$SOURCE" > "$WORK/Sabotaged.swift"
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: any prompt counts as empty"
sed 's/return line == "❯" || line == ">"/return true/' "$SOURCE" > "$WORK/Sabotaged.swift"
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: the words are taken for the question wherever the pointer is"
sed 's/guard t.hasPrefix("❯") else { continue }/_ = t/' "$SOURCE" > "$WORK/Sabotaged.swift"
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: a choice is found by the start of its name alone"
sed 's/if rest.isEmpty || rest.first == " " || rest.first == "(" || rest.first == "…" { return n }/return n/' "$SOURCE" > "$WORK/Sabotaged.swift"
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi

echo "sabotage: Codex's footer read without its effort"
sed 's/private static let effortPattern = "minimal|low|medium|high|xhigh|max|ultra|none"/private static let effortPattern = "none"/' "$SOURCE" > "$WORK/Sabotaged.swift"
grep -q 'effortPattern = "none"' "$WORK/Sabotaged.swift" || { echo "sabotage did not apply"; exit 1; }
build "$WORK/Sabotaged.swift" || exit 1
if "$WORK/test" >/dev/null; then echo "NOT CAUGHT"; exit 1; else echo "caught"; fi
