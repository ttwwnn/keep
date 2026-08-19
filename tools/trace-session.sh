#!/bin/bash
# Record a keep session: user input, window geometry, focus and render rate,
# on one merged timeline. The app side needs a Debug build of Keep.app.
#
# Usage:  tools/trace-session.sh [output.log]
# Stop with ctrl-c; the merged log path is printed at the end.
#
# Two halves, merged by timestamp:
#  - inside:  the app itself, launched with KEEP_TRACE=1 (see Trace.swift)
#  - outside: tools/keeptrace, a CGEventTap + window-server observer.
#    It counts keystrokes but NEVER records which keys: this is a terminal.

set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-/tmp/keep-session.log}"
APP="$ROOT/apps/macos/build/Build/Products/Debug/Keep.app"
TRACER="$ROOT/tools/.keeptrace-bin"
TMP="$(mktemp -d)"

[ -d "$APP" ] || { echo "build the app first: cd apps/macos && xcodebuild -project Keep.xcodeproj -scheme Keep -configuration Debug -derivedDataPath build build"; exit 1; }
[ -x "$TRACER" ] && [ "$TRACER" -nt "$ROOT/tools/keeptrace.swift" ] \
  || swiftc -O "$ROOT/tools/keeptrace.swift" -o "$TRACER" || exit 1

osascript -e 'quit app "Keep"' >/dev/null 2>&1
pkill -x Keep >/dev/null 2>&1
sleep 2

KEEP_TRACE=1 "$APP/Contents/MacOS/Keep" > "$TMP/app.log" 2>&1 &
APP_PID=$!
sleep 3
"$TRACER" > "$TMP/obs.log" 2>&1 &
OBS_PID=$!

cleanup() {
  kill "$OBS_PID" 2>/dev/null
  sleep 0.5
  sort -m -k1,1 "$TMP/obs.log" "$TMP/app.log" 2>/dev/null | grep -v '^$' > "$OUT"
  echo
  echo "=== session recorded: $OUT ($(wc -l < "$OUT" | tr -d ' ') lines) ==="
  awk '{print $3}' "$OUT" | sort | uniq -c | sort -rn | head -15 | sed 's/^/  /'
  rm -rf "$TMP"
  exit 0
}
trap cleanup INT TERM

echo "Recording. Use Keep normally; ctrl-c to stop. (The app keeps running.)"
tail -f "$TMP/obs.log" 2>/dev/null &
wait "$APP_PID" 2>/dev/null
cleanup
