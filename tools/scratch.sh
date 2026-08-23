# Kills that cannot reach past the test that made them.
#
# Sourced by the test scripts. Every one of these exists because the blunt
# version of it went wrong: `pkill -x cat` took every cat on the machine,
# including one in a shell of the person's own; `pkill -f "keep keys --tab"`
# took their attached sessions; `pkill -f keepd` took the daemon holding all
# of their work, which is the one process here that is meant to outlive
# everything else.
#
# The rule these follow: a test may kill what a test started, found by walking
# down from a pid it holds — never by matching a name against the whole
# machine.

# Every process descended from one, depth first.
descendants() {
    local pid=$1 kid
    for kid in $(pgrep -P "$pid" 2>/dev/null); do
        printf '%s\n' "$kid"
        descendants "$kid"
    done
}

# Kill descendants of $1 whose command line matches $2. Nothing outside that
# tree is a candidate, whatever it is called.
kill_descendants_matching() {
    local root=$1 pattern=$2 pid command
    for pid in $(descendants "$root"); do
        command=$(ps -o command= -p "$pid" 2>/dev/null) || continue
        case "$command" in
            *"$pattern"*) kill "$pid" 2>/dev/null ;;
        esac
    done
}

# Refuse to run against the daemon the person is actually using.
#
# Every one of these tests stands up a daemon of its own on a scratch socket,
# and each does it by exporting KEEP_SOCKET before starting anything. If that
# export were ever missed, the test would quietly drive the real daemon and
# then kill it on the way out.
require_scratch_socket() {
    local real
    real="${TMPDIR:-/tmp}/keep-${USER:-default}.sock"
    real=${real//\/\//\/}
    if [ -z "${KEEP_SOCKET:-}" ]; then
        printf '%s\n' "refusing to run: KEEP_SOCKET is not set, so this would use the real daemon"
        exit 1
    fi
    if [ "$KEEP_SOCKET" = "$real" ]; then
        printf '%s\n' "refusing to run: KEEP_SOCKET is the real daemon's socket"
        exit 1
    fi
}

# Give the person their app back, pointed at their own daemon.
#
# The test's own instance is pointed at a socket that is about to stop
# existing, so it cannot simply be left running — but leaving nothing at all
# means the test quietly closed an app somebody was working in.
restore_app() {
    local bundle=$1
    ( unset KEEP_SOCKET KEEP_TRACE; open "$bundle" >/dev/null 2>&1 ) &
}

# Put the client and daemon that were just built inside the app bundle.
#
# The app runs `Contents/Resources/keep`, not the one in `target/`, and
# nothing in the Xcode build copies it there. So a test that carefully builds
# the client and then starts the app is testing whichever client was last
# copied in by hand — which, when this was written, was three days old and had
# none of the behaviour under test. The copy invalidates the bundle's
# signature, hence the re-sign.
bundle_binaries() {
    local bundle=$1
    cp target/release/keep target/release/keepd "$bundle/Contents/Resources/" || return 1
    codesign --force --deep --sign - "$bundle" >/dev/null 2>&1
}

# Take the running app away, and be sure it is actually gone.
#
# A test that has just finished starts the person's app again with `open`,
# which returns long before the app is on screen. Start the next test
# immediately and its `pkill` can land in that gap: the app then appears
# halfway through, takes the focus, and the keystrokes the test believed it
# was sending to its own window are typed into somebody's session instead.
# What that looks like from the outside is a check failing for no reason.
stop_app() {
    pkill -x Keep 2>/dev/null
    local waited=0
    while pgrep -x Keep >/dev/null 2>&1 && [ "$waited" -lt 20 ]; do
        sleep 0.5
        waited=$((waited + 1))
        pkill -x Keep 2>/dev/null
    done
    # A late `open` from the test before this one still has a moment to fire.
    sleep 2
    pkill -x Keep 2>/dev/null
    sleep 1
}
