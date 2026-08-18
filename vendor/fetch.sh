#!/usr/bin/env bash
# Fetch libghostty-vt from the upstream release.
#
# Upstream publishes a prebuilt, universal xcframework, so building it
# yourself is optional: no Zig toolchain is required to work on this repo.
#
# Upstream has no semantic versioning yet — the only tag is `tip` — and the
# C API is explicitly unstable. Pin deliberately and re-run the ABI tests
# (cargo test -p keep-vt) after every bump.
set -euo pipefail

TAG="${GHOSTTY_TAG:-tip}"
URL="https://github.com/ghostty-org/ghostty/releases/download/${TAG}/ghostty-vt.xcframework.zip"
DEST="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/libghostty-vt"
SLICE="macos-arm64_x86_64"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "==> downloading $URL"
curl -fsSL "$URL" -o "$work/vt.zip"

echo "==> extracting $SLICE"
unzip -q "$work/vt.zip" -d "$work"
src="$work/ghostty-vt.xcframework/$SLICE"
[ -d "$src" ] || { echo "slice $SLICE not found in xcframework" >&2; exit 1; }

rm -rf "$DEST"
mkdir -p "$DEST/include"
cp "$src/libghostty-vt.a" "$DEST/"
cp -R "$src/Headers/." "$DEST/include/"

echo "==> ok: $DEST"
lipo -info "$DEST/libghostty-vt.a" 2>/dev/null || true
