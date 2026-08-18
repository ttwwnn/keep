#!/usr/bin/env bash
# Build GhosttyKit.xcframework — the full libghostty, including the Metal
# renderer and font stack. Unlike libghostty-vt there is no published binary,
# so this one really does need a Zig toolchain.
#
# Requires: zig (version pinned by ghostty's .zigversion), Xcode, and the
# Metal toolchain component:
#     xcodebuild -downloadComponent MetalToolchain
set -euo pipefail

TAG="${GHOSTTY_TAG:-tip}"
DEST="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "==> cloning ghostty ($TAG)"
git clone --depth 1 https://github.com/ghostty-org/ghostty.git "$work/ghostty"

echo "==> building xcframework (a few minutes)"
( cd "$work/ghostty" && zig build -Dxcframework-target=native -Doptimize=ReleaseFast )

rm -rf "$DEST/GhosttyKit.xcframework"
cp -R "$work/ghostty/macos/GhosttyKit.xcframework" "$DEST/"
echo "==> ok: $DEST/GhosttyKit.xcframework"
