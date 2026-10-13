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
    crates/keep/src/keeper.rs 's/let snapshot = mirror.terminal.handover_then(Some(keyboard), then);/let snapshot = { let _ = then; mirror.terminal.snapshot(keep_vt::Format::Vt).unwrap_or_default() };/'
sabotage "the keeper answers nothing" \
    crates/keep/src/keeper.rs 's/stand_in(\&mut mirror, shade.facts(\&dir), \&dir, \&reads);/let _ = (\&mut mirror, \&mut shade);/'
sabotage "the keeper does not read the clipboard for its program" \
    crates/keep/src/keeper.rs 's/let _ = mirror.terminal.read_clipboard_with(clipboard_reader(dir.to_path_buf(), Arc::clone(reads)));/let _ = (dir, reads);/'
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
sabotage "the client is raw only once it has the pane" \
    crates/keep/src/attach.rs 's/early_raw = RawGuard::enter(Arc::clone(\&alternate)).ok();/let _ = \&mut early_raw;/'
sabotage "a keeper that cannot be had is not told to go" \
    crates/keep/src/keeper.rs 's/    if taken.is_err() {/    if false \&\& taken.is_err() {/'
sabotage "the client says it took the pane before its mirror has it" \
    crates/keep/src/attach.rs 's/let taken = options.take.as_ref().map(|_| handed.is_some());/let taken = options.take.as_ref().map(|_| handed.is_some()); if keeping { if let Some(t) = taken { crate::keeper::tried_to_take(\&dir, t); } }/;s/        crate::keeper::tried_to_take(\&dir, taken);/        let _ = taken;/'
sabotage "the keeper hands over inside a sequence it cannot say the start of" \
    crates/keep/src/keeper.rs 's/if mirror.can_hand_over() || since.elapsed() >= Duration::from_secs(2) {/if true {/'
sabotage "a keeper giving up on a surface sits on the program's answers" \
    crates/keep/src/keeper.rs 's/^                pass_on(\&mut mirror, \&mut daemon, \&dir, \&mut background);/                let _ = \&background;/'
sabotage "every request for a surface in the same words" \
    crates/keep/src/keeper.rs 's/publish(\&dir, WAKE, format!("image {wakes}\\n").as_bytes());/publish(\&dir, WAKE, b"image\\n");/'
sabotage "the keeper waits for the app to answer a read of the clipboard" \
    crates/keep/src/keeper.rs 's|        // Answered later, by `answer_reads`.|        while !dir.join(CLIPBOARD_ANSWER).exists() \&\& dir.join(FLAG).exists() { std::thread::sleep(Duration::from_millis(20)); }|'
sabotage "a read unanswered when the pane is taken is not asked of the surface taking it" \
    crates/keep/src/keeper.rs 's/let unanswered = reads.lock().map(|r| r.unanswered()).unwrap_or_default();/let unanswered: Vec<u8> = Vec::new();/'
sabotage "an answer bigger than a frame sent in one" \
    crates/keep/src/keeper.rs 's/    for chunk in bytes.chunks(1 << 16) {/    for chunk in [bytes] {/'
sabotage "a keeper leaving leaves the app's file behind" \
    crates/keep/src/keeper.rs 's/CLIPBOARD_ANSWER, WAKE, SHOWING, ATTACH_FILE, APP_FILE,/CLIPBOARD_ANSWER, WAKE, SHOWING, ATTACH_FILE,/'
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
    's/let joined = row.wrapped \&\& row.reaches \&\& !printed(next).is_empty();/let joined = false \&\& row.wrapped \&\& row.reaches \&\& !printed(next).is_empty();/'
unit "a wrap kept over a next row since erased" keep-vt crates/keep-vt/src/handover.rs \
    's/let joined = row.wrapped \&\& row.reaches \&\& !printed(next).is_empty();/let joined = row.wrapped \&\& row.reaches \&\& { let _ = next; true };/'
unit "a wrap kept over a row since erased whole" keep-vt crates/keep-vt/src/handover.rs \
    's/let joined = row.wrapped \&\& row.reaches \&\& /let joined = row.wrapped \&\& /'
unit "the alternate screen's rows written inside the program's margins" keep-vt crates/keep-vt/src/handover.rs \
    's/out.extend_from_slice(Self::AS_IF_FRESH);/out.extend_from_slice(b"\\x1b[H");/'
unit "the cursor counted from the screen's corner under origin mode" keep-vt crates/keep-vt/src/handover.rs \
    's/_ => rest.extend_from_slice(\&if origin { within_margins(tail) } else { tail.to_vec() }),/_ => rest.extend_from_slice(\&{ let _ = origin; tail.to_vec() }),/'
unit "the cursor ESC 7 saved left out" keep-vt crates/keep-vt/src/handover.rs \
    's/let saved = self.saved_cursor();/let saved: Vec<u8> = Vec::new();/'
unit "the cursor ESC 7 saved on the alternate screen left out" keep-vt crates/keep-vt/src/handover.rs \
    's/let alternate_saved = self.saved_cursor();/let alternate_saved: Vec<u8> = Vec::new();/'
unit "the links in the history left out" keep-vt crates/keep-vt/src/handover.rs \
    's/let rendered = if row.linked || row.misdrawn {/let rendered = if row.misdrawn {/'
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
    's/^            let mut rest = modes;$/            let mut rest = modes.clone(); let written = [modes, written].concat();/'
unit "the blank rows at the bottom left out of the handover" keep-vt crates/keep-vt/src/handover.rs \
    's/let total = self.total_rows().max(1);/let total = lines.len().max(1);/'
unit "the blank rows at the bottom left out of the daemon's repaint" keep-vt crates/keep-vt/src/lib.rs \
    's/let below = b"\\r\\n".repeat(self.total_rows().saturating_sub(written));/let below: Vec<u8> = { let _ = written; Vec::new() };/'
unit "an image split between two writes goes unseen" keep crates/keep/src/keeper.rs \
    's/\*carried = data\[data.len() - keep..\].to_vec();/let _ = keep;/'
unit "a linked row moved along by the cursor after a wrap" keep-vt crates/keep-vt/src/handover.rs \
    's/out.extend(std::iter::repeat_n(b. ., blanks));/out.extend_from_slice(format!("\\x1b[{}G", x + 1).as_bytes()); let _ = blanks;/'
unit "a cursor saved waiting to wrap over an emptied cell put back not waiting" keep-vt crates/keep-vt/src/handover.rs \
    's/let wrap_over_nothing = self.get_bool(ffi::DATA_CURSOR_PENDING_WRAP)/let wrap_over_nothing = false \&\& self.get_bool(ffi::DATA_CURSOR_PENDING_WRAP)/'
unit "a cursor outside the margins under origin mode put inside them" keep-vt crates/keep-vt/src/handover.rs \
    's/Some(place) if origin \&\& scratch \&\& place.outside(self.dec_mode(69)) => {/Some(place) if false \&\& origin \&\& scratch \&\& place.outside(self.dec_mode(69)) => {/'
unit "a line feed after the last row" keep-vt crates/keep-vt/src/handover.rs \
    's/let Some((next_row, next)) = written.get(y + 1) else { break };/let Some((next_row, next)) = written.get(y + 1) else { out.extend_from_slice(b"\\r\\n"); break };/'
unit "colour with no text written as a blank row" keep-vt crates/keep-vt/src/handover.rs \
    's/            if colour \&\& !text {/            if false \&\& colour \&\& !text {/'
unit "the colour of the rows at the bottom left out" keep-vt crates/keep-vt/src/handover.rs \
    's/let rendered = if row.linked || row.misdrawn {/let rendered = if y < lines.len() \&\& (row.linked || row.misdrawn) {/'
unit "a colour given as RGB left out of a cell with no text" keep-vt crates/keep-vt/src/handover.rs \
    's/format!("\\x1b\[48;2;{};{};{}m", rgb.r, rgb.g, rgb.b).into_bytes()/{ let _ = rgb; Vec::new() }/'
unit "a colour given by palette left out of a cell with no text" keep-vt crates/keep-vt/src/handover.rs \
    's/format!("\\x1b\[48;5;{index}m").into_bytes()/{ let _ = index; Vec::new() }/'
unit "empty cells written in the style of the text before them" keep-vt crates/keep-vt/src/handover.rs \
    's/                    if gap {/                    if false \&\& gap {/'
unit "a link run on over the empty cells after it" keep-vt crates/keep-vt/src/handover.rs \
    's/                if !facts.content() || uri(\&next) != link {/                if uri(\&next) != link {/'
unit "the links of a row never asked" keep-vt crates/keep-vt/src/handover.rs \
    's/let uri = |cell: \&ffi::GridRef| if linked { Self::uri(cell) } else { Vec::new() };/let uri = |cell: \&ffi::GridRef| { let _ = (linked, cell); Vec::new() };/'
unit "a run of colour only asked of the formatter" keep-vt crates/keep-vt/src/handover.rs \
    's/            let body = if run.iter().any(|cell| cell.text) {/            let body = if true {/'
unit "a cell's colour taken from the cell before it" keep-vt crates/keep-vt/src/handover.rs \
    's/                    if pen.as_ref().map(|(raw, _)| \*raw) != Some(facts.raw) {/                    if pen.is_none() {/'
unit "a wide character's wait at the margin not gone to" keep-vt crates/keep-vt/src/handover.rs \
    's/                out.extend_from_slice(format!("\\x1b\[{cols}G").as_bytes());/                let _ = cols;/'
unit "no stand-in for a wide character the next row no longer starts with" keep-vt crates/keep-vt/src/handover.rs \
    's/                if !next_row.starts_wide {/                if false \&\& !next_row.starts_wide {/'
unit "the colour of the handover's own scroll left on a row it broke into" keep-vt crates/keep-vt/src/handover.rs \
    's/            if wrapped_into \&\& (!row.reaches || row.spacer_head) {/            if false \&\& wrapped_into {/'
unit "that colour left before a wide character's wait at the margin" keep-vt crates/keep-vt/src/handover.rs \
    's/            if wrapped_into \&\& (!row.reaches || row.spacer_head) {/            if wrapped_into \&\& !row.reaches {/'
unit "the cursor in use waiting over an emptied cell put back not waiting" keep-vt crates/keep-vt/src/handover.rs \
    's/            out.push(b. .);/            let _ = \&mut out;/'
unit "a linked cell at the margin written again without its link" keep-vt crates/keep-vt/src/handover.rs \
    's/        if uri.is_empty() || again.is_empty() || !words\[after..\].starts_with(\&again) {/        if true || uri.is_empty() || again.is_empty() {/'
unit "OSC 52 naming another clipboard left unanswered" keep-vt crates/keep-vt/src/effects.rs \
    's/        Some(reader) => reader(kind),/        Some(reader) if request.location == ffi::CLIPBOARD_LOCATION_STANDARD => reader(kind), Some(_) => ClipboardAnswer::Nothing,/'
unit "a big clipboard read left unanswered" keep-vt crates/keep-vt/src/effects.rs \
    's/if effects.events.replies.len() + len + 1 <= CLIPBOARD_REPLY_LIMIT {/if effects.events.replies.len() + len + 1 <= REPLY_LIMIT {/'
unit "the Kitty clipboard protocol answered" keep-vt crates/keep-vt/src/effects.rs \
    's/        if bytes.starts_with(b"\\x1b\]5522;") {/        if false \&\& bytes.starts_with(b"\\x1b]5522;") {/'
unit "the Kitty and iTerm2 protocols putting text on the clipboard" keep-vt crates/keep-vt/src/effects.rs \
    's/if write.location != ffi::CLIPBOARD_LOCATION_STANDARD || effects.osc != Some(52) {/if write.location != ffi::CLIPBOARD_LOCATION_STANDARD {/'
unit "the Kitty clipboard protocol's paste events said spoken" keep-vt crates/keep-vt/src/effects.rs \
    's/effects.events.replies.extend_from_slice(b"\\x1b\[?5522;0$y");/effects.events.replies.extend_from_slice(bytes);/'
unit "a read in the Kitty clipboard protocol given the clipboard" keep-vt crates/keep-vt/src/effects.rs \
    's/if effects.osc != Some(52) {/if false {/'
unit "a clipboard read answered with BEL where a surface ends with ST" keep-vt crates/keep-vt/src/effects.rs \
    's/let body = bytes.strip_suffix(b"\\x07");/let body: Option<\&[u8]> = None;/'
unit "a program asking whether images are spoken taken for one showing one" keep crates/keep/src/keeper.rs \
    "s/!matches!(action, b'q' | b'd')/{ let _ = action; true }/"
unit "the surface's word on dark and light not heard" keep crates/keep/src/keeper.rs \
    's/self.heard.store(report\[7\] - b.0., Ordering::Release);/let _ = report;/'
unit "the answer to the client's own question passed on to the program" keep crates/keep/src/keeper.rs \
    's/if owed > 0 \&\& self.owed.compare_exchange/if false \&\& owed > 0 \&\& self.owed.compare_exchange/'
unit "a live mirror keeps the images its surface draws" keep crates/keep/src/keeper.rs \
    's/        let _ = terminal.set_image_storage_limit(0);/        let _ = \&mut terminal;/'
unit "a keeper keeps no images, and leaves the protocol unanswered" keep crates/keep/src/keeper.rs \
    's/        let _ = mirror.terminal.set_image_storage_limit(limit);/        let _ = limit;/'
unit "XTGETTCAP TN left unanswered" keep-vt crates/keep-vt/src/effects.rs \
    's/        self.set(ffi::OPT_TERMINFO_NAME, \&name as \*const ffi::GhosttyString as \*const c_void)?;/        let _ = name;/'
unit "the terminfo entry not Ghostty's unless the app says" keep crates/keep/src/keeper.rs \
    's/let mut facts = Facts { terminfo: Some("xterm-ghostty".to_string()), ..Facts::default() };/let mut facts = Facts::default();/'
unit "the reader not told which clipboard the program named" keep-vt crates/keep-vt/src/effects.rs \
    "s/        Some(reader) => reader(kind),/        Some(reader) => reader(b'c'),/"
unit "a read of the clipboard put to the app before the one before it is answered" keep crates/keep/src/keeper.rs \
    's/        if reads.queue.len() == 1 {/        if true {/'
unit "the attach file's options not read" keep crates/keep/src/keeper.rs \
    's/"mirror" => options.mirror = value.parse().ok().filter(|\&n: \&usize| n > 0),/"mirror" => {}/'

say ""
say "sabotages caught: $CAUGHT, missed: $MISSED"
[ "$MISSED" -eq 0 ]
