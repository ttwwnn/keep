#!/usr/bin/env bash
# Build the macOS app's vendor from the pinned ghostty commit, with Zig:
# GhosttyKit.xcframework (the full libghostty: Metal renderer, fonts) and
# libghostty-vt. The same commands the kit-mac installer uses when it has no
# prebuilt copy, for a Mac that has Xcode and its Metal toolchain.
#
#   vendor/build-macos-vendor.sh [vendor-dir]
set -euo pipefail

COMMIT="${GHOSTTY_COMMIT:-a53771af0165c34a7bd753ddbcbc5dd7126ee87f}"
DEST="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
command -v zig >/dev/null || { echo "zig is required (0.16.0)" >&2; exit 1; }

if ! xcodebuild -showComponent MetalToolchain 2>/dev/null | grep -q 'Status: installed'; then
    echo "==> Metal toolchain"
    xcodebuild -downloadComponent MetalToolchain
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
echo "==> ghostty $COMMIT"
git -C "$work" init -q ghostty
git -C "$work/ghostty" fetch -q --depth 1 https://github.com/ghostty-org/ghostty.git "$COMMIT"
git -C "$work/ghostty" checkout -q --detach FETCH_HEAD
src="$work/ghostty"

echo "==> GhosttyKit.xcframework"
(cd "$src" && zig build -Doptimize=ReleaseFast -Dxcframework-target=native -Demit-macos-app=false)
rm -rf "$DEST/GhosttyKit.xcframework"
cp -R "$src/macos/GhosttyKit.xcframework" "$DEST/"

echo "==> libghostty-vt"
(cd "$src" && rm -rf zig-out && zig build -Doptimize=ReleaseFast -Demit-lib-vt=true -Demit-xcframework=false)
rm -rf "$DEST/libghostty-vt"
mkdir -p "$DEST/libghostty-vt/include"
cp "$src/zig-out/lib/libghostty-vt.a" "$DEST/libghostty-vt/"
cp -R "$src/zig-out/include/." "$DEST/libghostty-vt/include/"
printf 'module GhosttyVt {\n    umbrella header "ghostty/vt.h"\n    export *\n}\n' > "$DEST/libghostty-vt/include/module.modulemap"
echo "==> ok: $DEST"
