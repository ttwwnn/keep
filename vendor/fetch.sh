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

# ------------------------------------------------------------------- linux
# Upstream ships no prebuilt Linux artifact — only the source tarball — so
# the Linux slices are built here, with Zig, which cross-compiles from
# macOS without a container. `-Demit-lib-vt` keeps it to the library;
# `-Di18n=false` keeps gettext out of a build that only wants the .a.
if [ "${KEEP_LINUX:-1}" = "1" ]; then
    command -v zig >/dev/null 2>&1 || {
        echo "==> zig not found; skipping the Linux slices" >&2
        exit 0
    }
    SRC_URL="https://github.com/ghostty-org/ghostty/releases/download/${TAG}/libghostty-vt-source.tar.gz"
    echo "==> downloading $SRC_URL"
    curl -fsSL "$SRC_URL" -o "$work/vt-src.tar.gz"
    mkdir -p "$work/src"
    tar -xzf "$work/vt-src.tar.gz" -C "$work/src"
    SRC_DIR=$(find "$work/src" -maxdepth 1 -mindepth 1 -type d | head -1)

    for ARCH in x86_64 aarch64; do
        echo "==> building linux-$ARCH"
        (cd "$SRC_DIR" && rm -rf zig-out && zig build \
            -Demit-lib-vt=true -Di18n=false \
            -Dtarget="$ARCH-linux-gnu" -Doptimize=ReleaseFast)
        LDEST="$(dirname "$DEST")/libghostty-vt-linux-$ARCH"
        rm -rf "$LDEST"
        mkdir -p "$LDEST/include"
        cp "$SRC_DIR/zig-out/lib/libghostty-vt.a" "$LDEST/"
        cp -R "$SRC_DIR/zig-out/include/." "$LDEST/include/"
        echo "==> ok: $LDEST"
    done
fi
