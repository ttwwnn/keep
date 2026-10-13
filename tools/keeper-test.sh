#!/usr/bin/env bash
#
# A tab kept by its client while the app has no surface for it
# (crates/keep/src/keeper.rs, crates/keep-vt/src/handover.rs): the client
# binary driven the way the app drives it, through a pty per surface, against
# a daemon of its own. No app, no window, nothing typed anywhere but in the
# test's own tab. See tools/keeper-test/driver.py for the checks.
#
#   tools/keeper-test.sh             the check
#   tools/keeper-test.sh --sabotage  and then each way of getting it wrong,
#                                    built into a client of its own, which
#                                    the check must fail
set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=tools/scratch.sh
. tools/scratch.sh

say() { printf '%s\n' "$*"; }

cargo build --release -p keep -p keepd >/dev/null 2>&1 || { say "cargo build failed"; exit 1; }
cargo build --release -p keep-vt --example replay >/dev/null 2>&1 || { say "could not build the replay"; exit 1; }
REPLAY=target/release/examples/replay

python3 tools/keeper-test/driver.py target/release/keep target/release/keepd "$REPLAY"
status=$?
[ "${1:-}" = "--sabotage" ] || exit $status
[ $status -eq 0 ] || { say "the check itself fails; nothing to sabotage"; exit 1; }

# Each sabotage: a copy of the tree with one change, built into a target
# directory of its own (incremental after the first), whose client the
# check must catch.
SAB=$(mktemp -d /tmp/keeper-sabotage-XXXXXX)
trap 'rm -rf "$SAB"' EXIT
rsync -a --exclude target --exclude .git --exclude apps --exclude vendor/GhosttyKit.xcframework ./ "$SAB/tree/"
CAUGHT=0
MISSED=0
# The copy made the tree again before each sabotage. A file put back keeps
# the tree's time, older than the build of the sabotage before: cargo takes
# its crate to be up to date, and that sabotage would be built into the next
# one's client, caught for the wrong reason. Each file put back is touched.
restore() {
    rsync -ai --exclude target --exclude .git --exclude apps ./crates/ "$SAB/tree/crates/" |
        awk '$1 ~ /^>f/ { $1 = ""; sub(/^ /, ""); print }' |
        while IFS= read -r file; do touch "$SAB/tree/crates/$file"; done
}
sabotage() {  # sabotage <what> <file> <sed expression>
    local what=$1 file=$2 expression=$3
    say ""
    say "sabotage: $what"
    restore
    sed -i '' "$expression" "$SAB/tree/$file"
    if cmp -s "$file" "$SAB/tree/$file"; then
        say "  the sabotage changed nothing"; MISSED=$((MISSED + 1)); return
    fi
    if ! (cd "$SAB/tree" && CARGO_TARGET_DIR="$SAB/target" cargo build --release -p keep >/dev/null 2>&1); then
        say "  the sabotaged client does not build"; MISSED=$((MISSED + 1)); return
    fi
    if KEEPER_FAIL_FAST=1 python3 tools/keeper-test/driver.py "$SAB/target/release/keep" target/release/keepd "$REPLAY" >"$SAB/out" 2>&1; then
        say "  NOT CAUGHT"; MISSED=$((MISSED + 1))
    else
        grep -m4 -E 'FAIL|lines:|wanted|got' "$SAB/out" | sed 's/^/  /'
        say "  caught"; CAUGHT=$((CAUGHT + 1))
    fi
}

say ""
say "sabotage: the app asks for no mirror (the client of before)"
if KEEPER_FAIL_FAST=1 python3 tools/keeper-test/driver.py target/release/keep target/release/keepd "$REPLAY" --no-mirror >"$SAB/out" 2>&1; then
    say "  NOT CAUGHT"; MISSED=$((MISSED + 1))
else
    grep -m3 'FAIL' "$SAB/out" | sed 's/^/  /'
    say "  caught"; CAUGHT=$((CAUGHT + 1))
fi

sabotage "the client goes with its surface, whatever the app said" \
    crates/keep/src/attach.rs 's/            if flagged {/            if false \&\& flagged {/'
sabotage "the keeper hands over a repaint, not the whole terminal" \
    crates/keep/src/keeper.rs 's/let snapshot = mirror.terminal.handover(Some(keyboard));/let snapshot = mirror.terminal.snapshot(keep_vt::Format::Vt).unwrap_or_default();/'
sabotage "the keeper answers nothing" \
    crates/keep/src/keeper.rs 's/stand_in(\&mut mirror, shade.facts(\&dir), \&dir, app);/let _ = (\&mut mirror, \&mut shade);/'
sabotage "the keeper does not read the clipboard for its program" \
    crates/keep/src/keeper.rs 's/let _ = mirror.terminal.read_clipboard_with(clipboard_reader(dir.to_path_buf(), app));/let _ = (dir, app);/'
sabotage "the keeper answers a program the config denies the clipboard to" \
    crates/keep/src/keeper.rs 's/return ClipboardAnswer::Nothing;/return ClipboardAnswer::Text(Vec::new());/'
sabotage "the surface coming back attaches afresh instead of taking the connection" \
    crates/keep/src/attach.rs 's/Some((sock, screen)) => (sock, Some(screen)),/Some((_sock, screen)) => { let mut sock = Stream::connect(socket).context("connect to daemon")?; ClientMsg::Attach { workspace: name.to_string(), tab, cols, rows }.write(\&mut sock)?; (sock, Some(screen)) }/'
sabotage "the handover leaves out the main screen under the program" \
    crates/keep-vt/src/handover.rs 's/out.extend_from_slice(\&under.rows);/let _ = \&under.rows;/'
sabotage "the keeper outlives the app" \
    crates/keep/src/keeper.rs 's/if !dir.join(FLAG).exists() || app.map(|pid| !alive(pid)).unwrap_or(false) {/if !dir.join(FLAG).exists() {/'
sabotage "the keeper holds on to the surface's terminal" \
    crates/keep/src/keeper.rs 's/unsafe { libc::dup2(null.as_raw_fd(), fd) };/let _ = (\&null, fd);/'
sabotage "the scheme the surface was heard to say outlasts the app's word" \
    crates/keep/src/keeper.rs 's/if self.told.is_some() {/if false {/'
sabotage "a program that asked to be told of the scheme is not" \
    crates/keep/src/keeper.rs 's/if mirror.terminal.dec_mode(2031) {/if false {/'
sabotage "the client coming for a pane does not wait for its keeper" \
    crates/keep/src/keeper.rs 's/Err(error) if Instant::now() >= deadline || !dir.join(FLAG).exists() => return Err(error),/Err(error) => return Err(error),/'
sabotage "the client says nothing of having taken the pane" \
    crates/keep/src/attach.rs 's/crate::keeper::tried_to_take(\&dir, taken);/let _ = taken;/'
sabotage "the keeper feeds an image to its mirror instead of asking for a surface" \
    crates/keep/src/keeper.rs 's/if held.is_none() \&\& !bytes.is_empty() \&\& mirror.would_place_image(\&bytes) {/if false {/'
sabotage "the client dies with its surface while it waits to take the pane" \
    crates/keep/src/attach.rs 's/unsafe { libc::signal(libc::SIGHUP, libc::SIG_IGN) };/{}/;s/let mut mirror = if keeping {/if keeping { unsafe { libc::signal(libc::SIGHUP, libc::SIG_IGN) }; } let mut mirror = if keeping {/'
sabotage "the client's question written into the middle of the program's sequence" \
    crates/keep/src/attach.rs 's/if was_showing \&\& !showing \&\& asked_scheme {/if was_showing \&\& !showing \&\& asked_scheme { let _ = out.write_all(scheme.ask());/'
sabotage "the handover leaves out the start of a sequence it cut" \
    crates/keep-vt/src/handover.rs 's/out.extend_from_slice(\&continuation);/let _ = continuation;/'
sabotage "a keeper leaving takes the whole directory" \
    crates/keep/src/keeper.rs 's/let _ = std::fs::remove_dir(dir);/let _ = std::fs::remove_dir_all(dir);/'
sabotage "a client keeps wherever it was run" \
    crates/keep/src/keeper.rs 's/dir.parent().and_then(|p| p.file_name()).map(|n| n == "keep-attach").unwrap_or(false)/{ let _ = dir; true }/'

# And the rules the end-to-end run does not look at closely enough to tell —
# a wrap, a link, a colour, the cursor's shape — each caught by the unit
# tests of the crate it is in.
unit() {  # unit <what> <crate> <file> <sed expression>
    local what=$1 crate=$2 file=$3 expression=$4
    say ""
    say "sabotage: $what"
    restore
    sed -i '' "$expression" "$SAB/tree/$file"
    if cmp -s "$file" "$SAB/tree/$file"; then
        say "  the sabotage changed nothing"; MISSED=$((MISSED + 1)); return
    fi
    if ! (cd "$SAB/tree" && CARGO_TARGET_DIR="$SAB/target" cargo test -p "$crate" --no-run >"$SAB/out" 2>&1); then
        say "  the sabotaged crate does not build"; MISSED=$((MISSED + 1)); return
    fi
    if (cd "$SAB/tree" && CARGO_TARGET_DIR="$SAB/target" cargo test -p "$crate" >"$SAB/out" 2>&1); then
        say "  NOT CAUGHT"; MISSED=$((MISSED + 1))
    else
        grep -m2 -E 'panicked|^error' "$SAB/out" | sed 's/^/  /'
        say "  caught"; CAUGHT=$((CAUGHT + 1))
    fi
}
unit "soft wraps written as line feeds" keep-vt crates/keep-vt/src/handover.rs \
    's/wrapped \&\& self.reaches_margin(y as u32, cols)/false \&\& self.reaches_margin(y as u32, cols)/'
unit "a wrap kept over a next row since erased" keep-vt crates/keep-vt/src/handover.rs \
    's/ \&\& !visible(lines\[y + 1\]).is_empty();/;/'
unit "a wrap kept over a row since erased whole" keep-vt crates/keep-vt/src/handover.rs \
    's/wrapped \&\& self.reaches_margin(y as u32, cols) \&\& !visible/wrapped \&\& !visible/'
unit "the alternate screen's rows written inside the program's margins" keep-vt crates/keep-vt/src/handover.rs \
    's/out.extend_from_slice(Self::AS_IF_FRESH);/out.extend_from_slice(b"\\x1b[H");/'
unit "the cursor counted from the screen's corner under origin mode" keep-vt crates/keep-vt/src/handover.rs \
    's/rest.extend_from_slice(\&if origin { within_margins(tail) } else { tail.to_vec() });/rest.extend_from_slice(\&{ let _ = origin; tail.to_vec() });/'
unit "the cursor ESC 7 saved left out" keep-vt crates/keep-vt/src/handover.rs \
    's/let saved = self.saved_cursor();/let saved: Vec<u8> = Vec::new();/'
unit "the cursor ESC 7 saved on the alternate screen left out" keep-vt crates/keep-vt/src/handover.rs \
    's/let alternate_saved = self.saved_cursor();/let alternate_saved: Vec<u8> = Vec::new();/'
unit "the links in the history left out" keep-vt crates/keep-vt/src/handover.rs \
    's/let linked_line = if linked { self.linked_row(y as u32, cols) } else { None };/let linked_line: Option<Vec<u8>> = None;/'
unit "the colours a program set left out" keep-vt crates/keep-vt/src/handover.rs \
    's/let colours = self.colour_overrides();/let colours: Vec<u8> = Vec::new();/'
unit "the cursor's shape left out" keep-vt crates/keep-vt/src/handover.rs \
    's/format!("\\x1b\[{code} q").into_bytes()/{ let _ = code; Vec::new() }/'
unit "the modes a surface starts with differently left to the defaults" keep-vt crates/keep-vt/src/handover.rs \
    's/for mode in Self::DIFFERENT_DEFAULTS {/for mode in [0u16; 0] {/'
unit "only the top of the keyboard stack" keep-vt crates/keep-vt/src/handover.rs \
    's/Some(stack) if stack.len() > 1 \&\& stack.last() == Some(\&current) => {/Some(stack) if false \&\& stack.len() > 1 => {/'
unit "CSI = c answered" keep-vt crates/keep-vt/src/effects.rs \
    's/self.events.replies.truncate(before);/let _ = before;/'
unit "the modes before the rows" keep-vt crates/keep-vt/src/handover.rs \
    's/let mut written = self.rows_with_wraps_and_links(\&rows).unwrap_or(rows.clone());/let mut written = modes.clone(); written.extend_from_slice(\&self.rows_with_wraps_and_links(\&rows).unwrap_or(rows.clone()));/'
unit "the blank rows at the bottom left out of the handover" keep-vt crates/keep-vt/src/handover.rs \
    's/let trailing = self.total_rows().saturating_sub(split_rows(\&rows).len());/let trailing = 0usize;/'
unit "the blank rows at the bottom left out of the daemon's repaint" keep-vt crates/keep-vt/src/lib.rs \
    's/let below = b"\\r\\n".repeat(self.total_rows().saturating_sub(written));/let below: Vec<u8> = { let _ = written; Vec::new() };/'
unit "an image split between two writes goes unseen" keep crates/keep/src/keeper.rs \
    's/\*carried = data\[data.len() - keep..\].to_vec();/let _ = keep;/'
unit "the Kitty clipboard protocol's paste events said spoken" keep-vt crates/keep-vt/src/effects.rs \
    's/effects.events.replies.extend_from_slice(b"\\x1b\[?5522;0$y");/effects.events.replies.extend_from_slice(bytes);/'
unit "a read in the Kitty clipboard protocol given the clipboard" keep-vt crates/keep-vt/src/effects.rs \
    's/if effects.osc != Some(52) {/if false {/'
unit "a clipboard read answered with BEL where a surface ends with ST" keep-vt crates/keep-vt/src/effects.rs \
    's/if let Some(body) = bytes.strip_suffix(b"\\x07") {/if let Some(body) = None::<\&[u8]> {/'
unit "a program asking whether images are spoken taken for one showing one" keep crates/keep/src/keeper.rs \
    "s/!matches!(action, b'q' | b'd')/{ let _ = action; true }/"
unit "the surface's word on dark and light not heard" keep crates/keep/src/keeper.rs \
    's/self.heard.store(report\[7\] - b.0., Ordering::Release);/let _ = report;/'
unit "the answer to the client's own question passed on to the program" keep crates/keep/src/keeper.rs \
    's/if owed > 0 \&\& self.owed.compare_exchange/if false \&\& owed > 0 \&\& self.owed.compare_exchange/'
unit "the attach file's options not read" keep crates/keep/src/keeper.rs \
    's/"mirror" => options.mirror = value.parse().ok().filter(|\&n: \&usize| n > 0),/"mirror" => {}/'

say ""
say "sabotages caught: $CAUGHT, missed: $MISSED"
[ "$MISSED" -eq 0 ]
