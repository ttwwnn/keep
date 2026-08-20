#!/usr/bin/env bash
#
# What reaches the shell when you press a key.
#
# The app's input path had no test and broke three times without anything
# noticing: backspace arrived as escape, escape as F2, the left arrow as
# return, and ctrl-c as an escape sequence the shell printed instead of
# obeying. Every one of those was found by a person using the app.
#
# This drives the real app with real key events and asks the daemon what the
# far end received. Nothing is mocked: a daemon of its own on a scratch
# socket, a window, a shell, and CGEvents posted to the app's process so the
# window manager's opinion about focus never enters into it.
#
# The probe is `stty raw -echo; cat -v`, which turns every byte it is handed
# into something printable — 0x7f into ^?, escape into ^[ — so an assertion
# is a string comparison against the screen the daemon holds. Cases that need
# the far end to have asked for something first, like bracketed paste, put
# the request in front of the probe.
#
#   tools/keys-test.sh
#
# Not covered: mouse reporting and focus events. Both need a click or a focus
# change at a known place on screen, and the window manager here moves the
# window out from under one and refuses it the keyboard. They are the two
# remaining ways this app and the far end could disagree about a protocol.
#
# It stops the running Keep and does not start it again: this is a test, and
# the app it leaves behind would be pointed at a socket that no longer exists.

set -uo pipefail
cd "$(dirname "$0")/.."

APP=apps/macos/build/Build/Products/Debug/Keep.app
BIN=$APP/Contents/MacOS/Keep
# Short, because a unix socket path has about a hundred characters and the
# scratch directories this runs from have more.
SOCKET=/tmp/keep-keys-$$.sock
WORK=$(mktemp -d /tmp/keep-keys-XXXXXX)
SENDKEY=$WORK/sendkey
PASSED=0
FAILED=0

cleanup() {
    [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null
    # By name, never by process group: a script's background jobs share its
    # group, so killing the group kills the script and whatever ran it.
    [ -n "${PROBE_PID:-}" ] && kill "$PROBE_PID" 2>/dev/null
    pkill -f "keep keys --tab" 2>/dev/null
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
    rm -rf "$WORK" "$SOCKET"
}
trap cleanup EXIT

say() { printf '%s\n' "$*"; }

[ -x "$BIN" ] || { say "no app at $BIN — build it first"; exit 1; }

say "building the client and the daemon"
cargo build --release -p keep -p keepd >/dev/null 2>&1 || { say "cargo build failed"; exit 1; }
swiftc -O tools/sendkey.swift -o "$SENDKEY" 2>/dev/null || { say "could not build sendkey"; exit 1; }

export KEEP_SOCKET=$SOCKET
rm -f "$SOCKET"
./target/release/keepd >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
sleep 2

./target/release/keep new keys >/dev/null 2>&1 || { say "could not make a workspace"; exit 1; }

say "starting the app on its own daemon"
pkill -x Keep 2>/dev/null
sleep 2
KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
APP_PID=$!
sleep 11

# A fresh probe per case: the screen is cleared, whatever the case needs is
# asked for, and then the shell stops interpreting and starts printing.
start_probe() {
    local setup=${1:-}
    [ -n "${PROBE_PID:-}" ] && kill "$PROBE_PID" 2>/dev/null
    pkill -f "keep keys --tab" 2>/dev/null
    # The probe lives in the daemon's shell, not in the client that started
    # it, so letting go of the client leaves it running — and the next case's
    # setup gets typed into it and printed rather than run.
    pkill -x "cat" 2>/dev/null
    sleep 1
    { ( sleep 2
        printf 'stty sane; clear; %s stty raw -echo; cat -v\n' "$setup"
        sleep 45
      ) | script -q /dev/null ./target/release/keep keys --tab 1; } >/dev/null 2>&1 &
    PROBE_PID=$!
    sleep 6
}

# Joined, not line by line: the probe prints without newlines, so what looks
# like several lines on screen is one run of bytes wrapped at the width.
screen() {
    python3 tools/screen.py keys 1 2>/dev/null | tail -6 | tr -d "\n"
}

# A key may have more than one right answer. A cursor key is written one way
# when the program asked for application mode and another when it did not,
# and both mean the same arrow — so a case passes if any of its forms arrived.
check() {
    local what=$1 screen=$2
    shift 2
    local form
    for form in "$@"; do
        if [[ $screen == *"$form"* ]]; then
            say "  ok    $what → $form"
            PASSED=$((PASSED + 1))
            return
        fi
    done
    say "  FAIL  $what"
    say "        wanted one of: $*"
    say "        got: $(printf '%s' "$screen" | tail -1)"
    FAILED=$((FAILED + 1))
}

# ---------------------------------------------------------------- the keys

say ""
say "keys"
start_probe
"$SENDKEY" "$APP_PID" key 36 ; sleep 0.3      # return
"$SENDKEY" "$APP_PID" key 53 ; sleep 0.3      # escape
"$SENDKEY" "$APP_PID" key 51 ; sleep 0.3      # backspace
"$SENDKEY" "$APP_PID" key 123; sleep 0.3      # left arrow
"$SENDKEY" "$APP_PID" key 8 ctrl; sleep 0.5   # ctrl-c
KEYS=$(screen)
check "return"     "$KEYS" '^M'
check "escape"     "$KEYS" '^['
check "backspace"  "$KEYS" '^?'
check "left arrow" "$KEYS" '^[[D' '^[OD'
check "ctrl-c"     "$KEYS" '^C'

# ------------------------------------------------------- keys with a modifier

say ""
say "keys with a modifier"
start_probe
"$SENDKEY" "$APP_PID" key 123 alt  ; sleep 0.4   # alt-left, a word back
"$SENDKEY" "$APP_PID" key 36 shift ; sleep 0.4   # shift-return
"$SENDKEY" "$APP_PID" key 51 alt   ; sleep 0.5   # alt-backspace, a word away
MODS=$(screen)
check "alt-left"      "$MODS" '^[[1;3D' '^[b' '^[^[[D'
check "shift-return"  "$MODS" '^[^M' '^M' '^[[13;2u'
check "alt-backspace" "$MODS" '^[^?' '^W' '^[[127;3u'

# ------------------------------------------------------------ bracketed paste

say ""
say "bracketed paste"
printf 'PASTED-BY-THE-TEST' | pbcopy
start_probe "printf '\\033[?2004h';"
"$SENDKEY" "$APP_PID" key 9 cmd; sleep 1.2       # cmd-v
PASTE=$(screen)
check "paste arrives" "$PASTE" 'PASTED-BY-THE-TEST'
check "paste is wrapped, so a shell cannot run what is in it" "$PASTE" '^[[200~'

# ---------------------------------------------------------------------- done

say ""
if [ "$FAILED" -eq 0 ]; then
    say "$PASSED checks passed"
    exit 0
fi
say "$FAILED of $((PASSED + FAILED)) checks failed"
exit 1
