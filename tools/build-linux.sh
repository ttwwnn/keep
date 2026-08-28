#!/usr/bin/env bash
#
# The Linux build, from a Mac, in one command.
#
#   tools/build-linux.sh            # both architectures
#   tools/build-linux.sh x86_64     # just one
#
# No container and no Linux box is needed to *build*: Zig cross-compiles
# libghostty-vt from upstream's source tarball, and cargo-zigbuild uses the
# same Zig as the linker for the Rust. The daemon and client are portable
# already — the socket prefers XDG_RUNTIME_DIR, the terminfo probe knows the
# Linux paths — so the whole port is this build.
#
# Proving it does take Linux: `docker run --rm -v "$PWD:/w" debian:stable
# /w/dist/linux-<arch>/keepd` — or the test suites, cross-compiled with
# `cargo zigbuild --tests --target <arch>-unknown-linux-gnu` and run the
# same way.
set -euo pipefail
cd "$(dirname "$0")/.."

command -v zig >/dev/null || { echo "zig is required (brew install zig)"; exit 1; }
command -v cargo-zigbuild >/dev/null || {
    echo "cargo-zigbuild is required (brew install cargo-zigbuild)"; exit 1
}

ARCHES=${1:-"x86_64 aarch64"}
for ARCH in $ARCHES; do
    VENDOR="vendor/libghostty-vt-linux-$ARCH"
    if [ ! -f "$VENDOR/libghostty-vt.a" ]; then
        echo "==> $VENDOR missing; building it (vendor/fetch.sh builds all slices)"
        ./vendor/fetch.sh
    fi
    TARGET="$ARCH-unknown-linux-gnu"
    rustup target list --installed | grep -q "$TARGET" || rustup target add "$TARGET"
    echo "==> building keep + keepd for $TARGET"
    cargo zigbuild --release --target "$TARGET" -p keep -p keepd
    DIST="dist/linux-$ARCH"
    mkdir -p "$DIST"
    cp "target/$TARGET/release/keep" "target/$TARGET/release/keepd" "$DIST/"
    echo "==> $DIST"
done
