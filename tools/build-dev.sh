#!/usr/bin/env bash
#
# The build the tests drive: KeepDev.
#
# Same source, different name. The suites stop the app, kill it, drag its
# windows around the screen and quit it, and every one of those is fine done
# to a build of their own and ruinous done to the app somebody is working in.
# Since the window server tells apps apart by name, the only way for a test to
# be sure whose window it is counting is for the two to have different ones.
#
# What the name buys, all of it a consequence of that one setting:
#   - `pkill -x KeepDev` cannot reach a running Keep;
#   - the mouse tool counts windows owned by "KeepDev" and no others;
#   - `frontmost of process "KeepDev"` focuses the build under test;
#   - the app's state lives in Application Support/KeepDev, so a test cannot
#     rearrange somebody's sidebar or reopen their windows.
#
# The bundle id differs too, or Launch Services treats the two as one app and
# activating either brings up whichever it saw first.
#
#   tools/build-dev.sh
#
# Fast when nothing changed: both builds here are incremental.
set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

say() { printf '%s\n' "$*"; }

cargo build --release -p keep -p keepd >/dev/null 2>&1 || {
    say "cargo build failed"; exit 1
}

# `-derivedDataPath build-dev` and not `build`: the person's app was launched
# out of `build`, and rebuilding into it swaps the binaries under a running
# process — harmless until it relaunches, and then a surprise.
(
    cd apps/macos
    xcodebuild -project Keep.xcodeproj -scheme Keep -configuration Debug \
        -derivedDataPath build-dev \
        PRODUCT_NAME=KeepDev \
        PRODUCT_BUNDLE_IDENTIFIER=dev.luhw.keep.dev \
        build
) >/tmp/keep-build-dev.log 2>&1 || {
    say "xcodebuild failed; the tail of /tmp/keep-build-dev.log:"
    tail -20 /tmp/keep-build-dev.log
    exit 1
}

# The app runs the client inside its own bundle, and nothing in the Xcode
# build puts it there. Skip this and the app under test quietly uses whichever
# client was last copied in by hand.
bundle_binaries "$(dev_app)" || { say "could not put the fresh binaries in the bundle"; exit 1; }

say "built $(dev_app)"
