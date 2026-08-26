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
# It drives KeepDev, a build of its own (tools/build-dev.sh), so it can be run
# while somebody is working in Keep: different name, different bundle id,
# different state directory, and every kill, focus and window count in here
# goes by that name. It still takes the mouse over for a couple of minutes.

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

APP=$(dev_app)
APP_NAME=$(app_name "$APP")
export KEEP_APP_NAME=$APP_NAME
BIN=$APP/Contents/MacOS/$APP_NAME
# Short, because a unix socket path has about a hundred characters and the
# scratch directories this runs from have more.
SOCKET=/tmp/keep-keys-$$.sock
WORK=$(mktemp -d /tmp/keep-keys-XXXXXX)
SENDKEY=$WORK/sendkey
PASSED=0
FAILED=0

cleanup() {
    # Whatever the person had copied, they still have.
    if [ -n "${CLIPBOARD_TAKEN:-}" ]; then
        printf '%s' "${CLIPBOARD:-}" | pbcopy 2>/dev/null
    fi
    [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null
    # By name, never by process group: a script's background jobs share its
    # group, so killing the group kills the script and whatever ran it.
    [ -n "${PROBE_PID:-}" ] && kill "$PROBE_PID" 2>/dev/null
    # Only the clients this script started. The person may be attached to
    # their own daemon from a shell somewhere, and that is not ours to close.
    kill_descendants_matching $$ "keep keys --tab"
    [ -n "${DAEMON_PID:-}" ] && kill "$DAEMON_PID" 2>/dev/null
    # KEEP_KEEP_LOGS=1 leaves the app's own trace behind. What a key does
    # inside the app is not visible from the far end: a chord libghostty turns
    # into an action this app does not implement arrives as nothing at all,
    # and only the trace says so.
    if [ -n "${KEEP_KEEP_LOGS:-}" ]; then
        say ""
        say "logs kept in $WORK"
    else
        rm -rf "$WORK"
    fi
    rm -f "$SOCKET"
}
# INT and TERM as well as EXIT: a test that is interrupted has still
# taken the person's app away, and leaving it taken is how a stopped
# test looks exactly like a broken app.
trap cleanup EXIT INT TERM

say() { printf '%s\n' "$*"; }

say "building $APP_NAME, the client, the daemon and the tools (yours is left alone)"
./tools/build-dev.sh >/dev/null || { say "could not build $APP_NAME — run tools/build-dev.sh"; exit 1; }

swiftc -O tools/sendkey.swift -o "$SENDKEY" 2>/dev/null || { say "could not build sendkey"; exit 1; }

export KEEP_SOCKET=$SOCKET
# What the app remembers between launches goes in here too, so this test
# neither reads the arrangement of whoever is running it nor leaves its
# own behind.
export KEEP_STATE_DIR=$WORK/state
mkdir -p "$KEEP_STATE_DIR"
require_scratch_socket
rm -f "$SOCKET"
./target/release/keepd >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
sleep 2

./target/release/keep new keys >/dev/null 2>&1 || { say "could not make a workspace"; exit 1; }

# The app is a single instance pointed at one socket, so the one the person
# is using has to step aside for the one under test. It is started again on
# the way out, pointed back at their own daemon — which itself is never
# touched: it goes on holding their sessions throughout.
say "starting $APP_NAME on its own daemon"
stop_app
KEEP_TRACE=1 "$BIN" >"$WORK/app.log" 2>&1 &
APP_PID=$!
sleep 11
place_on_screen

# A fresh probe per case: the screen is cleared, whatever the case needs is
# asked for, and then the shell stops interpreting and starts printing.
start_probe() {
    local setup=${1:-}
    [ -n "${PROBE_PID:-}" ] && kill "$PROBE_PID" 2>/dev/null
    kill_descendants_matching $$ "keep keys --tab"
    # The probe lives in the daemon's shell, not in the client that started
    # it, so letting go of the client leaves it running — and the next case's
    # setup gets typed into it and printed rather than run. Reached by walking
    # down from this test's own daemon: every shell it is running is one this
    # test asked for, and no cat anywhere else on the machine is.
    kill_descendants_matching "$DAEMON_PID" "cat"
    sleep 1
    { ( sleep 2
        printf 'stty sane; clear; %s stty raw -echo; cat -vt\n' "$setup"
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

# What a key must NOT have sent.
#
# The checks above ask whether the right bytes arrived, which a key can manage
# while also sending bytes nobody wanted. `cat -v` writes any byte over 127 as
# M-something, and none of the keys pressed here has any business producing
# one — so its presence is a fault whatever arrived beside it.
#
# Known limit: a plain shell asks for nothing, and a terminal tells a program
# only what it asked to be told. Arrows leaving a glyph behind inside a
# program that turned the kitty keyboard protocol on is a real report that
# this check does NOT reproduce, because this probe never turns it on. Adding
# a probe that does is the missing half of this file.
reject() {
    local what=$1 screen=$2 form=$3
    if [[ $screen == *"$form"* ]]; then
        say "  FAIL  $what"
        say "        must not have sent: $form"
        FAILED=$((FAILED + 1))
    else
        say "  ok    $what"
        PASSED=$((PASSED + 1))
    fi
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
"$SENDKEY" "$APP_PID" key 44 shift; sleep 0.5   # shift-slash, which is "?"
check "shift-slash is a question mark" "$(screen)" '?'
# Tab and its shifted twin. Programs that cycle a selection forwards with one
# and backwards with the other — Claude Code's mode switch among them — go
# quiet when the shifted one arrives as a plain tab or as nothing at all,
# and from inside the program the two are the same silence.
start_probe
"$SENDKEY" "$APP_PID" key 48 shift; sleep 0.5   # shift-tab
check "shift-tab is a backtab" "$(screen)" '^[[Z'
# U+F700..U+F8FF is the block AppKit names function keys with; in UTF-8 every
# one of them begins EF 9C or EF 9D, which cat -v writes as M-oM-^ or M-oM-^].
reject "no key sent a glyph of its own" "$KEYS" 'M-o'

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

# ------------------------------- keys inside a program that asked for more
#
# A terminal tells a program only what that program asked to be told. A plain
# shell asks for nothing, so every case above is answered in the oldest
# encoding there is — which is why they can all pass while keys misbehave
# inside anything full-screen. Claude Code, vim and the rest turn the kitty
# keyboard protocol on, and in that mode the terminal reports the key, its
# modifiers and the text it carries, separately. This section asks what
# actually arrives there.

say ""
say "keys inside a program using the kitty keyboard protocol"

# One key per probe, and the screen read straight after it. Several keys into
# one screen cannot say which of them sent nothing.
KITTY_ON="printf '\033[>1u';"

start_probe "$KITTY_ON printf '\033[?u';"
say "    the terminal reports its flags as: $(screen)"

probe_key() {   # probe_key <label> <keycode> [modifier...]
    local label=$1
    shift
    start_probe "$KITTY_ON"
    "$SENDKEY" "$APP_PID" key "$@"
    sleep 0.8
    LAST=$(screen)
    say "    $label → $LAST"
}
# Checked, not just shown. These are the ones whose right answer is known,
# and the ones that were silently wrong: a question mark that arrived as a
# shifted slash, and a word-delete that arrived as nothing.
probe_key "left"          123
check "left, in kitty mode"           "$LAST" '^[[D'
probe_key "opt-left"      123 alt
check "opt-left, in kitty mode"       "$LAST" '^[b'
probe_key "opt-backspace" 51 alt
check "opt-backspace sends something" "$LAST" '^[^?' '^[[127;3u'
probe_key "plain slash"   44
check "slash, in kitty mode"          "$LAST" '/'
probe_key "shift-slash"   44 shift
check "shift-slash is a question mark, in kitty mode" "$LAST" '?'
# The one that sent people looking: a program in kitty mode cycling its modes
# forwards on tab and backwards on shift-tab does nothing at all on the
# second, because shift arrives spent. Either encoding is fine — the legacy
# backtab or the protocol's own — as long as the shift is still in it.
probe_key "shift-tab"     48 shift
check "shift-tab is still a backtab, in kitty mode" "$LAST" '^[[Z' '^[[9;2u'

# ------------------------------------------------------------ bracketed paste

say ""
say "bracketed paste"
# Taken and given back. A test that leaves the clipboard holding its own
# sentinel has reached outside itself, and one that reads the clipboard before
# it has won it reports on whatever the person last copied — and then prints
# it, which is worse.
CLIPBOARD=$(pbpaste 2>/dev/null)
CLIPBOARD_TAKEN=1
for _ in 1 2 3 4 5; do
    printf 'PASTED-BY-THE-TEST' | pbcopy
    [ "$(pbpaste 2>/dev/null)" = "PASTED-BY-THE-TEST" ] && break
    sleep 0.4
done
if [ "$(pbpaste 2>/dev/null)" != "PASTED-BY-THE-TEST" ]; then
    say "  SKIP  could not take the clipboard; the paste checks need it"
    SKIP_PASTE=1
fi
start_probe "printf '\\033[?2004h';"
"$SENDKEY" "$APP_PID" key 9 cmd; sleep 1.2       # cmd-v
PASTE=$(screen)
if [ -z "${SKIP_PASTE:-}" ]; then
    check "paste arrives" "$PASTE" 'PASTED-BY-THE-TEST'
    check "paste is wrapped, so a shell cannot run what is in it" "$PASTE" '^[[200~'
fi

# ---------------------------------------------------------------------- done

say ""
if [ "$FAILED" -eq 0 ]; then
    say "$PASSED checks passed"
    exit 0
fi
say "$FAILED of $((PASSED + FAILED)) checks failed"
exit 1
