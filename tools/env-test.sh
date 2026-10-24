#!/usr/bin/env bash
#
# The macOS app writes its own environment only before `ghostty_init`
# (apps/macos/Sources/Keep/Display/GhosttyApp.swift). libghostty keeps the
# environment it found there, and a variable added later can move it and free
# the old one under libghostty: opened from the Dock — a dozen variables, no
# LANG — the app read freed memory on its next config load and closed itself as
# it came up, every time (24/10/2026, `setenv("KEEP_RESTAURO_ESTADO")` at the
# first `keep restauro`). A child is told what it needs through its own
# environment (`KeepCLI.environment`).
#
# So the one write allowed is the one before `ghostty_init`, and this fails on
# any other. The app opened as the Dock opens it: tools/dock-launch-test.sh.
# No build, no window: seconds, and in CI.
#
#   tools/env-test.sh
set -uo pipefail
cd "$(dirname "$0")/.."

allowed='apps/macos/Sources/Keep/Display/GhosttyApp.swift:setenv("GHOSTTY_RESOURCES_DIR"'
found=$(grep -rnE '\b(setenv|unsetenv|putenv)[[:space:]]*\(' apps/macos/Sources \
    | grep -v '^[^:]*:[0-9]*:[[:space:]]*//')

status=0
while IFS= read -r line; do
    [ -n "$line" ] || continue
    file=${line%%:*}
    code=$(printf '%s' "$line" | cut -d: -f3- | sed -E 's/^[[:space:]]+//')
    case "$file:$code" in
        "$allowed"*) ;;
        *) printf 'FAIL  the app writes its environment: %s\n' "$line"; status=1 ;;
    esac
done <<<"$found"

# And the one allowed write still comes before `ghostty_init`.
source=apps/macos/Sources/Keep/Display/GhosttyApp.swift
write=$(grep -n 'setenv("GHOSTTY_RESOURCES_DIR"' "$source" | head -1 | cut -d: -f1)
init=$(grep -n 'ghostty_init(' "$source" | head -1 | cut -d: -f1)
if [ -n "$write" ] && { [ -z "$init" ] || [ "$write" -gt "$init" ]; }; then
    printf 'FAIL  GHOSTTY_RESOURCES_DIR is written after ghostty_init (lines %s, %s)\n' "$write" "$init"
    status=1
fi

[ $status -eq 0 ] && echo "ok    the app writes its environment only before ghostty_init"
exit $status
