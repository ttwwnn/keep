#!/usr/bin/env bash
#
# A folded workspace whose tab is on screen is marked as that tab would be
# (pedido de 27/11/2026): the header wears the chosen tab's capsule while its
# tabs are hidden under it, and gives it back to the tab when it opens.
#
# A workspace "Marca" with two tabs, in a daemon of the test's own. Open, the
# colour of the chosen tab's row is read off the window, beside its title, and
# the header's at the same x; folded (the sidebar state seeded, the app opened
# again), the header's must be the chosen tab's, and no longer its own.
#
#   tools/fold-mark-test.sh            CAPTURA=<file.png> saves the window, folded
#
# Your app and your daemon are never touched.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=$(dev_app)
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
SOCKET=/tmp/keep-mark-$$.sock
WORK=$(mktemp -d /tmp/keep-mark-XXXXXX)
PASSED=0
FAILED=0

cleanup() {
    [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
    [ -n "${KEEP_WORK:-}" ] && say "kept: $WORK" || rm -rf "$WORK"
    rm -f "$SOCKET"
}
trap cleanup EXIT INT TERM

say() { printf '%s\n' "$*"; }
check() {  # check <what> <wanted> <got>
    if [ "$3" = "$2" ]; then
        say "  ok    $1"
        PASSED=$((PASSED + 1))
    else
        say "  FAIL  $1"
        say "        wanted: $2"
        say "        got:    $3"
        FAILED=$((FAILED + 1))
    fi
}

if [ -z "${SKIP_BUILD:-}" ]; then
    say "building $APP_NAME, the client, the daemon and the tools (yours is left alone)"
    ./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }
fi
AXPRESS=$WORK/axpress
swiftc -O tools/axpress.swift -o "$AXPRESS" 2>/dev/null || { say "could not build axpress"; exit 1; }
# The colour of a point of the app's window, as drawn: "r g b" of the pixel at screen point (x, y).
cat >"$WORK/cor.swift" <<'SWIFT'
import CoreGraphics
import Foundation
import ImageIO
let a = CommandLine.arguments
let (name, x, y, png) = (a[1], Double(a[2])!, Double(a[3])!, a[4])
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
guard let w = list.first(where: { ($0[kCGWindowOwnerName as String] as? String) == name && ($0[kCGWindowLayer as String] as? Int) == 0 }),
      let id = w[kCGWindowNumber as String] as? Int,
      let b = w[kCGWindowBounds as String] as? [String: Double] else { exit(1) }
let p = Process()
p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
p.arguments = ["-x", "-o", "-l", String(id), png]
try p.run(); p.waitUntilExit()
guard let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: png) as CFURL, nil),
      let img = CGImageSourceCreateImageAtIndex(src, 0, nil) else { exit(1) }
let scale = Double(img.width) / b["Width"]!
let (px, py) = (Int((x - b["X"]!) * scale), Int((y - b["Y"]!) * scale))
var pixel = [UInt8](repeating: 0, count: 4)
let ctx = CGContext(data: &pixel, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
                    space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
ctx.draw(img, in: CGRect(x: -px, y: py - img.height + 1, width: img.width, height: img.height))
print(pixel[0], pixel[1], pixel[2])
SWIFT
swiftc -O "$WORK/cor.swift" -o "$WORK/cor" 2>/dev/null || { say "could not build cor"; exit 1; }

export KEEP_SOCKET=$SOCKET
export KEEP_STATE_DIR=$WORK/state
mkdir -p "$KEEP_STATE_DIR"
require_scratch_socket
rm -f "$SOCKET"
"$APP/Contents/Resources/keepd" >>"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
waited=0
while [ ! -S "$SOCKET" ] && [ "$waited" -lt 20 ]; do sleep 0.25; waited=$((waited + 1)); done
"$APP/Contents/Resources/keep" new Marca >/dev/null 2>&1
"$APP/Contents/Resources/keep" new Marca >/dev/null 2>&1

frame() { "$AXPRESS" "$APP_NAME" frame "$1" 2>/dev/null; }
mid_y() { frame "$1" | awk '{ print $2 + $4 / 2 }'; }
# The header's zzz is on the header's line; 30 points left of it the row is empty, header or tab alike.
left_of_zzz() { frame sleep-workspace-Marca | awk '{ print $1 - 30 }'; }
colour() { "$WORK/cor" "$APP_NAME" "$1" "$2" "$WORK/janela.png"; }
same() {  # same <rgb> <rgb>: yes if no channel differs by more than 6
    [ -n "$1" ] && [ -n "$2" ] || { echo no; return; }
    read -r r1 g1 b1 <<<"$1"; read -r r2 g2 b2 <<<"$2"
    d() { echo $(( $1 > $2 ? $1 - $2 : $2 - $1 )); }
    [ "$(d "$r1" "$r2")" -le 6 ] && [ "$(d "$g1" "$g2")" -le 6 ] && [ "$(d "$b1" "$b2")" -le 6 ] && echo yes || echo no
}
launch() {  # launch <folded: [] or ["Marca"]>
    stop_app
    printf '{"0":{"folded":%s,"isCollapsed":false,"verticalTabs":false,"width":280}}' "$1" >"$KEEP_STATE_DIR/sidebar-state.json"
    "$BIN" >>"$WORK/app.log" 2>&1 &
    APP_PID=$!
    waited=0
    while [ -z "$(frame sleep-workspace-Marca)" ] && [ "$waited" -lt 120 ]; do sleep 0.25; waited=$((waited + 1)); done
    place_on_screen
    osascript -e "tell application \"System Events\" to tell process \"$APP_NAME\" to set frontmost to true" >/dev/null 2>&1
    sleep 2
}

say ""
say "open: the chosen tab is marked, the header is not"
launch '[]'
chosen=""
for t in 1 2; do
    y=$(mid_y "sidebar-title-Marca/$t")
    [ -n "$y" ] || continue
    c=$(colour "$(left_of_zzz)" "$y")
    other=$(mid_y "sidebar-title-Marca/$((3 - t))")
    # The chosen one is the tab whose row differs from its sibling's.
    if [ -n "$other" ] && [ "$(same "$c" "$(colour "$(left_of_zzz)" "$other")")" = no ]; then
        # Of the two, the one that is not the bare ground the header shows.
        if [ "$(same "$c" "$(colour "$(left_of_zzz)" "$(mid_y sleep-workspace-Marca)")")" = no ]; then chosen=$c; fi
    fi
done
header_open=$(colour "$(left_of_zzz)" "$(mid_y sleep-workspace-Marca)")
check "a chosen tab's row is found, marked ($chosen)" yes "$([ -n "$chosen" ] && echo yes || echo no)"
check "the open header is not marked ($header_open)" no "$(same "$header_open" "$chosen")"

say ""
say "folded: the header wears the mark of its tab on screen"
launch '["Marca"]'
check "the tabs are hidden" "" "$(frame "sidebar-title-Marca/1")"
header_folded=$(colour "$(left_of_zzz)" "$(mid_y sleep-workspace-Marca)")
check "the folded header is marked as the chosen tab was ($header_folded)" yes "$(same "$header_folded" "$chosen")"
check "and no longer looks as it did open" no "$(same "$header_folded" "$header_open")"
if [ -n "${CAPTURA:-}" ]; then cp "$WORK/janela.png" "$CAPTURA" && say "  captured: $CAPTURA"; fi

say ""
say "open again: the mark goes back to the tab"
launch '[]'
check "the header is not marked any more" no "$(same "$(colour "$(left_of_zzz)" "$(mid_y sleep-workspace-Marca)")" "$chosen")"

say ""
say "$PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
