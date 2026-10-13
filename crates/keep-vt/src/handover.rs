//! The whole terminal, for another to carry on from.
//!
//! A repaint ([`Terminal::snapshot`]) is what a client attaching to a tab is
//! painted with: the screen in use, and as much history as the grid it was
//! made from kept. It says nothing of the main screen while a program has the
//! alternate one, breaks every soft-wrapped line in two, leaves the links out
//! of the history, and forgets the title, the colours a program set and the
//! cursor's shape. Enough to look at; not enough to hand a terminal over
//! without anybody noticing.
//!
//! [`Terminal::handover`] is the rest: both screens, every row of history with
//! its wraps and its links, and what the program set about the terminal
//! itself. It is what a tab the app has put to sleep gives the surface that
//! wakes it, the keep client having kept the tab's terminal in the meantime.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::{Terminal, ffi};

/// The Kitty keyboard flags each screen has pushed, bottom first, as the one
/// following the stream counted them: the library keeps a stack too, but tells
/// only what is on top.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keyboard {
    pub main: Vec<u8>,
    pub alternate: Vec<u8>,
}

/// One screen's part of a handover: its rows, and what comes after them —
/// the modes, the margins, the cursor and its pen.
struct ScreenPart {
    rows: Vec<u8>,
    rest: Vec<u8>,
}

impl Terminal {
    /// What a terminal that has seen nothing needs to be told to become this
    /// one: both screens, every row of history, the modes, the cursor, the
    /// keyboard, the title, the colours a program set.
    ///
    /// Reading the main screen from under the alternate one means going
    /// there: a terminal on the alternate screen is left on the main one,
    /// as the program would have left it. Feed the result to a fresh
    /// terminal to have one as before.
    pub fn handover(&mut self, keyboard: Option<&Keyboard>) -> Vec<u8> {
        // The start of a sequence the last write ended in the middle of, said
        // last, for the rest of it to complete there as it will here. Read
        // before anything is written to this terminal, which would end it.
        let continuation = self.continuation().unwrap_or_default();
        let title = self.title();
        let colours = self.colour_overrides();
        let mut out = Vec::new();
        // A fresh terminal on its main screen, cleared, with nothing switched
        // on that would change what a key sends.
        out.extend_from_slice(b"\x1b[?1049l\x1b[H\x1b[2J");
        for mode in Self::INPUT_MODES {
            if !self.dec_mode(mode) {
                out.extend_from_slice(format!("\x1b[?{mode}l").as_bytes());
            }
        }
        out.extend_from_slice(b"\x1b[>4m");
        // The modes a Ghostty surface and this library start out with
        // differently, said either way: the formatter writes only the modes
        // that differ from this library's defaults, and a mode at its default
        // here may not be one there. Before the rows, for 2027: how a cluster
        // of code points becomes cells is decided as they are written.
        for mode in Self::DIFFERENT_DEFAULTS {
            let on = self.dec_mode(mode);
            out.extend_from_slice(format!("\x1b[?{mode}{}", if on { 'h' } else { 'l' }).as_bytes());
        }

        match self.alternate_screen() {
            None => {
                let main = self.screen_handover();
                let keys = self.keyboard_for(keyboard.map(|k| k.main.as_slice()));
                let shape = self.cursor_shape();
                let saved = self.saved_cursor();
                out.extend_from_slice(&main.rows);
                out.extend_from_slice(&saved);
                out.extend_from_slice(&main.rest);
                out.extend_from_slice(&keys);
                out.extend_from_slice(&shape);
            }
            Some(mode) => {
                let alternate = self.screen_handover();
                let alternate_keys = self.keyboard_for(keyboard.map(|k| k.alternate.as_slice()));
                let alternate_shape = self.cursor_shape();
                // Out as the program would go: 1049 puts the cursor back where
                // it was on the main screen, 47 and 1047 never moved it.
                self.write(format!("\x1b[?{mode}l").as_bytes());
                let under = self.screen_handover();
                let main_keys = self.keyboard_for(keyboard.map(|k| k.main.as_slice()));
                let main_shape = self.cursor_shape();
                // What ESC 7 saved on the main screen: 1049 saved the cursor
                // over it on the way in, there as here, so only after 47 and
                // 1047 is there one to say.
                let main_saved = if mode == 1049 { Vec::new() } else { self.saved_cursor() };
                // Back to the alternate screen as it stands — 47 neither
                // clears it nor saves a cursor — for the one saved there.
                self.write(b"\x1b[?47h");
                let alternate_saved = self.saved_cursor();
                out.extend_from_slice(&under.rows);
                out.extend_from_slice(&main_saved);
                out.extend_from_slice(&under.rest);
                out.extend_from_slice(&main_keys);
                out.extend_from_slice(&main_shape);
                // And back in the way the program went, which saves that
                // cursor again. The main screen's part has put the program's
                // margins and modes back, and they are the terminal's, not a
                // screen's: the alternate screen's rows are written as onto a
                // fresh terminal, its own of each put back after them.
                out.extend_from_slice(format!("\x1b[?{mode}h").as_bytes());
                out.extend_from_slice(Self::AS_IF_FRESH);
                out.extend_from_slice(&alternate.rows);
                out.extend_from_slice(&alternate_saved);
                out.extend_from_slice(&alternate.rest);
                out.extend_from_slice(&alternate_keys);
                out.extend_from_slice(&alternate_shape);
            }
        }
        out.extend_from_slice(&colours);
        if !title.is_empty() {
            out.extend_from_slice(b"\x1b]2;");
            out.extend_from_slice(title.as_bytes());
            out.extend_from_slice(b"\x1b\\");
        }
        out.extend_from_slice(&continuation);
        out
    }

    /// What rows are written under, put back as a fresh terminal has it:
    /// origin mode off, no margins, no scrolling region, autowrap on, insert
    /// mode off, the pen and the protection off, every character set ASCII
    /// and G0 the one in use, the cursor home. The modes, the margins and the
    /// character sets came over from the main screen with the switch, and
    /// rows written under a program's scrolling region land inside it, over
    /// one another with autowrap off, pushed along with insert mode on, and
    /// as line drawing with its character set.
    const AS_IF_FRESH: &'static [u8] =
        b"\x1b[?6l\x1b[?69l\x1b[r\x1b[?7h\x1b[4l\x1b[0m\x1b[0\"q\x1b(B\x1b)B\x1b*B\x1b+B\x0f\x1b[H";

    /// The cursor ESC 7 last saved on the screen in use — where ESC 8 puts
    /// it back, with which pen, which character sets, in which origin mode
    /// — said and saved again, then let go of: what comes next says the
    /// cursor in use.
    ///
    /// Read by putting it back here, so only once everything else about the
    /// screen has been read. Where nothing was saved, ESC 8 goes home with
    /// nothing on, and so does this.
    fn saved_cursor(&mut self) -> Vec<u8> {
        self.write(b"\x1b8");
        let origin = self.dec_mode(6);
        let Some(cursor) = self.cursor_state() else { return Vec::new() };
        let mut out = Vec::with_capacity(cursor.len() + 48);
        // Said before the margins are, which leaves the whole screen the
        // region: from its top left, as from the screen's.
        if origin {
            out.extend_from_slice(b"\x1b[?6h");
        }
        out.extend_from_slice(&cursor);
        out.extend_from_slice(b"\x1b7");
        if origin {
            out.extend_from_slice(b"\x1b[?6l");
        }
        // What the cursor in use says of itself leaves out whatever is off:
        // the protection and the character sets saved are taken off again.
        out.extend_from_slice(b"\x1b[0m\x1b[0\"q\x1b(B\x1b)B\x1b*B\x1b+B\x0f");
        out
    }

    /// The cursor in use as the formatter says it — its place, the pen it
    /// writes with, its protection, the character sets — without the screen:
    /// formatted from one cell, the bottom left.
    fn cursor_state(&self) -> Option<Vec<u8>> {
        let rows = self.size().1;
        let cell = self.cell(0, rows.saturating_sub(1))?;
        let selection = ffi::Selection {
            size: std::mem::size_of::<ffi::Selection>(),
            start: cell,
            end: cell,
            rectangle: false,
        };
        let screen = ffi::ScreenExtra {
            size: std::mem::size_of::<ffi::ScreenExtra>(),
            cursor: true,
            style: true,
            hyperlink: false,
            protection: true,
            kitty_keyboard: false,
            charsets: true,
        };
        let options = ffi::FormatterOptions {
            size: std::mem::size_of::<ffi::FormatterOptions>(),
            emit: ffi::FORMAT_VT,
            unwrap: false,
            trim: false,
            extra: ffi::TerminalExtra {
                size: std::mem::size_of::<ffi::TerminalExtra>(),
                palette: false,
                modes: false,
                scrolling_region: false,
                tabstops: false,
                pwd: false,
                keyboard: false,
                screen,
            },
            selection: &selection as *const ffi::Selection as *const c_void,
        };
        let formatted = self.format_with(options).ok()?;
        // The cell's own text first, if it has any; the cursor from its
        // place on.
        let at = first_cursor_position(&formatted)?;
        Some(formatted[at..].to_vec())
    }

    /// Modes a Ghostty surface starts with set where this library starts with
    /// them reset — cursor blinking (12) and grapheme clustering (2027),
    /// measured against Keep's surfaces mode by mode (tools/hibernate-keep-test.sh
    /// asks both).
    pub const DIFFERENT_DEFAULTS: [u16; 2] = [12, 2027];

    /// The screen in use: every row, the history first, then the modes, then
    /// where the cursor is and what it writes with.
    ///
    /// The rows come before the modes, unlike in a repaint: a program that
    /// turned autowrap off would otherwise have the replayed rows overwrite
    /// one another at the margin instead of wrapping where they wrapped.
    fn screen_handover(&self) -> ScreenPart {
        let extras = self.format_with(self.options(true)).unwrap_or_default();
        let rows = self.format_with(self.options(false)).unwrap_or_default();
        let lead = leading_modes(&extras);
        let modes = without_screen_switches(&extras[..lead]);
        // The formatter leaves out the blank rows at the bottom. Replayed
        // without them, a screen with history ends one row short for each:
        // its last line lands on the bottom row, where the cursor is put back
        // — on that line, for the next output to overwrite, rather than on
        // the blank row below it.
        let trailing = self.total_rows().saturating_sub(split_rows(&rows).len());
        let below = b"\r\n".repeat(trailing);
        let origin = self.dec_mode(6);
        if extras[lead..].starts_with(&rows) {
            let mut written = self.rows_with_wraps_and_links(&rows).unwrap_or(rows.clone());
            written.extend_from_slice(&below);
            let mut rest = modes;
            let tail = &extras[lead + rows.len()..];
            rest.extend_from_slice(&if origin { within_margins(tail) } else { tail.to_vec() });
            ScreenPart { rows: written, rest }
        } else {
            // Not the shape this was written against: the rows as they come,
            // and the cursor put back by hand, which is right about
            // everything but wraps, links and the pen it writes with.
            let (x, y) = self.cursor();
            let mut written = rows;
            written.extend_from_slice(&below);
            let mut rest = modes;
            rest.extend_from_slice(format!("\x1b[{};{}H\x1b[0m", y + 1, x + 1).as_bytes());
            ScreenPart { rows: written, rest }
        }
    }

    /// The formatter's options: everything about the screen, or its rows
    /// alone. The palette is left out (it would be all 256 colours, the
    /// unchanged ones overriding the theme of whoever replays it; see
    /// `colour_overrides`), and so are the Kitty flags, said with their stack.
    fn options(&self, extras: bool) -> ffi::FormatterOptions {
        let screen = ffi::ScreenExtra {
            size: std::mem::size_of::<ffi::ScreenExtra>(),
            cursor: extras,
            style: extras,
            hyperlink: extras,
            protection: extras,
            kitty_keyboard: false,
            charsets: extras,
        };
        ffi::FormatterOptions {
            size: std::mem::size_of::<ffi::FormatterOptions>(),
            emit: ffi::FORMAT_VT,
            unwrap: false,
            trim: false,
            extra: ffi::TerminalExtra {
                size: std::mem::size_of::<ffi::TerminalExtra>(),
                palette: false,
                modes: extras,
                scrolling_region: extras,
                tabstops: extras,
                pwd: extras,
                keyboard: extras,
                screen,
            },
            selection: std::ptr::null(),
        }
    }

    /// The formatter's rows, one per row of the screen from the oldest kept,
    /// with each soft wrap kept as one — the row's last cell written and the
    /// next row's text following it, so that the margin breaks the line
    /// again, rather than a line feed breaking it for good — and each link
    /// in it, history included.
    ///
    /// The formatter ends every row it styles with a reset and starts every
    /// one from nothing, so a row can be swapped for another rendering of
    /// itself without touching its neighbours. Nil when the rows are not the
    /// screen's: then the repaint's rows stand.
    fn rows_with_wraps_and_links(&self, rows: &[u8]) -> Option<Vec<u8>> {
        let lines = split_rows(rows);
        if lines.len() > self.total_rows().max(1) {
            return None;
        }
        let cols = self.size().0;
        let mut out = Vec::with_capacity(rows.len());
        for (y, line) in lines.iter().enumerate() {
            let (wrapped, linked) = self.row_flags(y as u32)?;
            let linked_line = if linked { self.linked_row(y as u32, cols) } else { None };
            match linked_line {
                Some(text) if visible(&text) == visible(line) => out.extend_from_slice(&text),
                _ => out.extend_from_slice(line),
            }
            if y + 1 < lines.len() {
                // A soft wrap kept where the margin can break the line again:
                // the row written out to its last column, and something in
                // the next one to carry on with. A row whose end, or whose
                // next row, was erased after it wrapped keeps its wrap —
                // joined to it then, the next line would start where the
                // blanks begin, or a line feed after the empty row would
                // take it a row too far.
                let joined =
                    wrapped && self.reaches_margin(y as u32, cols) && !visible(lines[y + 1]).is_empty();
                if !joined {
                    out.extend_from_slice(b"\r\n");
                }
            }
        }
        Some(out)
    }

    /// Whether a row is written out to its last column: the formatter leaves
    /// out the blank cells a row ends with, and the next row's text goes
    /// past the margin, wrapping there, only from the last one.
    fn reaches_margin(&self, y: u32, cols: u16) -> bool {
        let Some(last) = self.screen_cell(cols.saturating_sub(1), y) else { return false };
        let mut cell: ffi::Cell = 0;
        if unsafe { ffi::ghostty_grid_ref_cell(&last, &mut cell) } != ffi::SUCCESS {
            return false;
        }
        let flag = |data: c_int| {
            let mut value = false;
            let rc = unsafe { ffi::ghostty_cell_get(cell, data, &mut value as *mut bool as *mut c_void) };
            rc == ffi::SUCCESS && value
        };
        let number = |data: c_int| {
            let mut value: c_int = 0;
            let rc = unsafe { ffi::ghostty_cell_get(cell, data, &mut value as *mut c_int as *mut c_void) };
            if rc == ffi::SUCCESS { value } else { 0 }
        };
        // Text, a colour, either half of a wide character, or the space one
        // too wide for the margin left there: written, all of them.
        flag(ffi::CELL_DATA_HAS_TEXT)
            || flag(ffi::CELL_DATA_HAS_STYLING)
            || number(ffi::CELL_DATA_WIDE) != ffi::CELL_WIDE_NARROW
            || number(ffi::CELL_DATA_CONTENT_TAG) >= 2
    }

    /// A cell anywhere on the screen in use, history included: row 0 is the
    /// oldest kept.
    pub(crate) fn screen_cell(&self, x: u16, y: u32) -> Option<ffi::GridRef> {
        let point = ffi::Point {
            tag: ffi::POINT_TAG_SCREEN,
            value: ffi::PointValue { coordinate: ffi::PointCoordinate { x, y } },
        };
        let mut cell = ffi::GridRef {
            size: std::mem::size_of::<ffi::GridRef>(),
            node: std::ptr::null_mut(),
            x: 0,
            y: 0,
        };
        let rc = unsafe { ffi::ghostty_terminal_grid_ref(self.raw, point, &mut cell) };
        (rc == ffi::SUCCESS && !cell.node.is_null()).then_some(cell)
    }

    /// Whether a row of the screen wraps into the next one, and whether any
    /// of its cells may be linked.
    fn row_flags(&self, y: u32) -> Option<(bool, bool)> {
        let first = self.screen_cell(0, y)?;
        let mut row: ffi::Row = 0;
        if unsafe { ffi::ghostty_grid_ref_row(&first, &mut row) } != ffi::SUCCESS {
            return None;
        }
        let flag = |data: c_int| {
            let mut value = false;
            let rc = unsafe { ffi::ghostty_row_get(row, data, &mut value as *mut bool as *mut c_void) };
            rc == ffi::SUCCESS && value
        };
        Some((flag(ffi::ROW_DATA_WRAP), flag(ffi::ROW_DATA_HYPERLINK)))
    }

    /// A row written run by run, each linked run inside its OSC 8, each run
    /// at its own column (a gap between two links is cells the formatter has
    /// nothing to say about). Starts and ends with the pen reset, as the
    /// formatter's own rows do.
    fn linked_row(&self, y: u32, cols: u16) -> Option<Vec<u8>> {
        let mut out = b"\x1b[0m".to_vec();
        let mut x: u16 = 0;
        while x < cols {
            let start = self.screen_cell(x, y)?;
            let uri = Self::uri(&start);
            let mut end_x = x;
            let mut end = start;
            while end_x + 1 < cols {
                let next = self.screen_cell(end_x + 1, y)?;
                if Self::uri(&next) != uri {
                    break;
                }
                end = next;
                end_x += 1;
            }
            let text = self.formatted_run(start, end).ok()?;
            if !text.is_empty() {
                out.extend_from_slice(format!("\x1b[{}G\x1b[0m", x + 1).as_bytes());
                if uri.is_empty() {
                    out.extend_from_slice(&text);
                } else {
                    out.extend_from_slice(b"\x1b]8;;");
                    out.extend_from_slice(&uri);
                    out.extend_from_slice(b"\x1b\\");
                    out.extend_from_slice(&text);
                    out.extend_from_slice(b"\x1b]8;;\x1b\\");
                }
            }
            x = end_x + 1;
        }
        out.extend_from_slice(b"\x1b[0m");
        Some(out)
    }

    /// The Kitty flags of the screen in use, as a stack when the one given
    /// agrees with the terminal about what is on top, as the top alone when
    /// not — always said, zero included, as a repaint says them.
    fn keyboard_for(&self, stack: Option<&[u8]>) -> Vec<u8> {
        let current = self.kitty_keyboard_flags();
        match stack {
            Some(stack) if stack.len() > 1 && stack.last() == Some(&current) => {
                let mut out = format!("\x1b[={};1u", stack[0]).into_bytes();
                for flags in &stack[1..] {
                    out.extend_from_slice(format!("\x1b[>{flags}u").as_bytes());
                }
                out
            }
            _ => format!("\x1b[={current};1u").into_bytes(),
        }
    }

    /// CSI q for the shape a program gave the cursor on the screen in use;
    /// nothing when it gave none, so that the shape is whoever replays it's
    /// own. Told apart by the default shape: a terminal made by
    /// [`Terminal::with_default_cursor_unset`] starts hollow, which no CSI q
    /// can ask for.
    fn cursor_shape(&self) -> Vec<u8> {
        let mut state: ffi::RenderState = std::ptr::null_mut();
        if unsafe { ffi::ghostty_render_state_new(std::ptr::null(), &mut state) } != ffi::SUCCESS {
            return Vec::new();
        }
        let mut style: c_int = ffi::CURSOR_STYLE_BLOCK_HOLLOW;
        let mut blinking = false;
        unsafe {
            if ffi::ghostty_render_state_update(state, self.raw) == ffi::SUCCESS {
                ffi::ghostty_render_state_get(
                    state,
                    ffi::RENDER_STATE_DATA_CURSOR_VISUAL_STYLE,
                    &mut style as *mut c_int as *mut c_void,
                );
                ffi::ghostty_render_state_get(
                    state,
                    ffi::RENDER_STATE_DATA_CURSOR_BLINKING,
                    &mut blinking as *mut bool as *mut c_void,
                );
            }
            ffi::ghostty_render_state_free(state);
        }
        let code = match style {
            ffi::CURSOR_STYLE_BLOCK => if blinking { 1 } else { 2 },
            ffi::CURSOR_STYLE_UNDERLINE => if blinking { 3 } else { 4 },
            ffi::CURSOR_STYLE_BAR => if blinking { 5 } else { 6 },
            _ => return Vec::new(),
        };
        format!("\x1b[{code} q").into_bytes()
    }

    /// Make the default cursor shape one no program can ask for, so that
    /// [`Terminal::handover`] can tell a shape a program chose from none.
    pub fn with_default_cursor_unset(self) -> Self {
        let hollow: c_int = ffi::CURSOR_STYLE_BLOCK_HOLLOW;
        unsafe {
            ffi::ghostty_terminal_set(
                self.raw,
                ffi::OPT_DEFAULT_CURSOR_STYLE,
                &hollow as *const c_int as *const c_void,
            );
        }
        self
    }

    /// OSC 10, 11, 12 and 4 for every colour a program changed: the rest are
    /// left to whoever replays this, whose theme they are.
    fn colour_overrides(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (code, data, default) in [
            (10, ffi::DATA_COLOR_FOREGROUND, ffi::DATA_COLOR_FOREGROUND_DEFAULT),
            (11, ffi::DATA_COLOR_BACKGROUND, ffi::DATA_COLOR_BACKGROUND_DEFAULT),
            (12, ffi::DATA_COLOR_CURSOR, ffi::DATA_COLOR_CURSOR_DEFAULT),
        ] {
            let Some(colour) = self.colour(data) else { continue };
            if self.colour(default) != Some(colour) {
                out.extend_from_slice(format!("\x1b]{code};{}\x1b\\", spec(colour)).as_bytes());
            }
        }
        if let (Some(palette), Some(defaults)) =
            (self.palette(ffi::DATA_COLOR_PALETTE), self.palette(ffi::DATA_COLOR_PALETTE_DEFAULT))
        {
            for (index, (colour, default)) in palette.iter().zip(defaults.iter()).enumerate() {
                if colour != default {
                    out.extend_from_slice(format!("\x1b]4;{index};{}\x1b\\", spec(*colour)).as_bytes());
                }
            }
        }
        out
    }

    /// The background as a program left it, or as the terminal began.
    pub fn background_colour(&self) -> Option<[u8; 3]> {
        self.colour(ffi::DATA_COLOR_BACKGROUND).map(|c| [c.r, c.g, c.b])
    }

    fn colour(&self, data: c_int) -> Option<ffi::ColorRgb> {
        let mut colour = ffi::ColorRgb::default();
        let rc = unsafe {
            ffi::ghostty_terminal_get(self.raw, data, &mut colour as *mut ffi::ColorRgb as *mut c_void)
        };
        (rc == ffi::SUCCESS).then_some(colour)
    }

    fn palette(&self, data: c_int) -> Option<Vec<ffi::ColorRgb>> {
        let mut palette = vec![ffi::ColorRgb::default(); 256];
        let rc = unsafe { ffi::ghostty_terminal_get(self.raw, data, palette.as_mut_ptr() as *mut c_void) };
        (rc == ffi::SUCCESS).then_some(palette)
    }
}

fn spec(colour: ffi::ColorRgb) -> String {
    format!("rgb:{:02x}/{:02x}/{:02x}", colour.r, colour.g, colour.b)
}

/// The rows of the formatter's output, which separates them with CR LF and
/// never writes either inside one.
fn split_rows(rows: &[u8]) -> Vec<&[u8]> {
    if rows.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at + 1 < rows.len() {
        if rows[at] == b'\r' && rows[at + 1] == b'\n' {
            lines.push(&rows[start..at]);
            at += 2;
            start = at;
        } else {
            at += 1;
        }
    }
    lines.push(&rows[start..]);
    lines
}

/// How long the run of mode and tab-stop sequences the formatter writes
/// before the rows is: CSI ending in h, l (modes), g, G or H (clearing tab
/// stops, moving along the row to set them, going home), and ESC H.
fn leading_modes(bytes: &[u8]) -> usize {
    let mut at = 0;
    while at + 1 < bytes.len() && bytes[at] == 0x1b {
        match bytes[at + 1] {
            b'H' => at += 2,
            b'[' => {
                let mut end = at + 2;
                while end < bytes.len() && (0x20..=0x3f).contains(&bytes[end]) {
                    end += 1;
                }
                if end >= bytes.len() || !matches!(bytes[end], b'h' | b'l' | b'g' | b'G' | b'H') {
                    break;
                }
                at = end + 1;
            }
            _ => break,
        }
    }
    at
}

/// The modes, less any that switch screens: which screen is in use is said
/// once, where it belongs, and a second 1049 would save the cursor again.
fn without_screen_switches(modes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(modes.len());
    let mut at = 0;
    while at < modes.len() {
        if modes[at] == 0x1b && modes.get(at + 1) == Some(&b'[') {
            let mut end = at + 2;
            while end < modes.len() && (0x20..=0x3f).contains(&modes[end]) {
                end += 1;
            }
            let params = &modes[at + 2..end.min(modes.len())];
            let switch = matches!(params, b"?47" | b"?1047" | b"?1049")
                && matches!(modes.get(end), Some(b'h') | Some(b'l'));
            let stop = (end + 1).min(modes.len());
            if !switch {
                out.extend_from_slice(&modes[at..stop]);
            }
            at = stop;
        } else {
            out.push(modes[at]);
            at += 1;
        }
    }
    out
}

/// Where the first CUP (`CSI row ; column H`) in `bytes` starts.
fn first_cursor_position(bytes: &[u8]) -> Option<usize> {
    let mut at = 0;
    while at + 1 < bytes.len() {
        if bytes[at] == 0x1b && bytes[at + 1] == b'[' {
            let mut end = at + 2;
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b';') {
                end += 1;
            }
            if bytes.get(end) == Some(&b'H') {
                return Some(at);
            }
            at = end;
        } else {
            at += 1;
        }
    }
    None
}

/// The formatter's last words about a screen — the margins, then where the
/// cursor is — with the cursor's place counted from the top left of the
/// margins, as origin mode reads it. The formatter counts it from the
/// screen's, as if origin mode were off, and a program with its margins
/// set and origin mode on got its cursor back that many rows and columns
/// further down and along.
fn within_margins(tail: &[u8]) -> Vec<u8> {
    let pair = |params: &[u8]| -> Option<(u16, u16)> {
        let text = std::str::from_utf8(params).ok()?;
        let (first, second) = text.split_once(';')?;
        Some((first.parse().ok()?, second.parse().ok()?))
    };
    let (mut top, mut left) = (0u16, 0u16);
    let mut at = 0;
    while at + 1 < tail.len() {
        if tail[at] != 0x1b {
            at += 1;
            continue;
        }
        match tail[at + 1] {
            b'[' => {
                let mut end = at + 2;
                while end < tail.len() && (0x20..=0x3f).contains(&tail[end]) {
                    end += 1;
                }
                let Some(&last) = tail.get(end) else { break };
                let params = &tail[at + 2..end];
                match last {
                    b'r' => top = pair(params).map(|(t, _)| t.saturating_sub(1)).unwrap_or(top),
                    b's' => left = pair(params).map(|(l, _)| l.saturating_sub(1)).unwrap_or(left),
                    b'H' => {
                        let Some((row, column)) = pair(params) else { break };
                        let mut out = tail[..at].to_vec();
                        out.extend_from_slice(
                            format!(
                                "\x1b[{};{}H",
                                row.saturating_sub(top).max(1),
                                column.saturating_sub(left).max(1)
                            )
                            .as_bytes(),
                        );
                        out.extend_from_slice(&tail[end + 1..]);
                        return out;
                    }
                    _ => {}
                }
                at = end + 1;
            }
            b']' => {
                // OSC 7, the directory: to its end, BEL or ST.
                let mut end = at + 2;
                while end < tail.len() {
                    if tail[end] == 0x07 {
                        end += 1;
                        break;
                    }
                    if tail[end] == 0x1b && tail.get(end + 1) == Some(&b'\\') {
                        end += 2;
                        break;
                    }
                    end += 1;
                }
                at = end;
            }
            _ => at += 2,
        }
    }
    tail.to_vec()
}

/// The text of a row with every escape sequence taken out, to tell two
/// renderings of one row apart from two different rows.
fn visible(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != 0x1b {
            out.push(bytes[at]);
            at += 1;
            continue;
        }
        match bytes.get(at + 1) {
            Some(b'[') => {
                let mut end = at + 2;
                while end < bytes.len() && (0x20..=0x3f).contains(&bytes[end]) {
                    end += 1;
                }
                at = end + 1;
            }
            Some(b']') => {
                // OSC, ended by BEL or ST.
                let mut end = at + 2;
                while end < bytes.len() {
                    if bytes[end] == 0x07 {
                        end += 1;
                        break;
                    }
                    if bytes[end] == 0x1b && bytes.get(end + 1) == Some(&b'\\') {
                        end += 2;
                        break;
                    }
                    end += 1;
                }
                at = end;
            }
            _ => at += 2,
        }
    }
    // Trailing blanks are the same row however written.
    while out.last() == Some(&b' ') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Format;

    /// A terminal that has seen nothing but the handover of one like `t`:
    /// handing over leaves a terminal changed, and `t` stays as it was, to
    /// be compared with.
    fn handed(t: &Recorded, keyboard: Option<&Keyboard>) -> Terminal {
        let mut copy = t.again();
        let (cols, rows) = copy.size();
        let bytes = copy.handover(keyboard);
        let mut fresh = blank(cols, rows);
        fresh.write(&bytes);
        fresh
    }

    /// A terminal as the client keeps one.
    fn blank(cols: u16, rows: u16) -> Terminal {
        let mut t = Terminal::new(cols, rows).unwrap().with_default_cursor_unset();
        t.set_scrollback_limit(10_000_000).unwrap();
        t.track_continuation(1 << 20).unwrap();
        t
    }

    fn mirror(cols: u16, rows: u16) -> Recorded {
        Recorded { terminal: blank(cols, rows), log: Vec::new() }
    }

    /// A terminal and everything written to it, from which another like it
    /// is made.
    struct Recorded {
        terminal: Terminal,
        log: Vec<u8>,
    }

    impl Recorded {
        fn write(&mut self, bytes: &[u8]) {
            self.terminal.write(bytes);
            self.log.extend_from_slice(bytes);
        }

        fn again(&self) -> Terminal {
            let (cols, rows) = self.terminal.size();
            let mut t = blank(cols, rows);
            t.write(&self.log);
            t
        }
    }

    impl std::ops::Deref for Recorded {
        type Target = Terminal;
        fn deref(&self) -> &Terminal {
            &self.terminal
        }
    }

    impl std::ops::DerefMut for Recorded {
        fn deref_mut(&mut self) -> &mut Terminal {
            &mut self.terminal
        }
    }

    fn vt(t: &Terminal) -> String {
        String::from_utf8_lossy(&t.snapshot(Format::Vt).unwrap()).into_owned()
    }

    fn plain(t: &Terminal) -> String {
        String::from_utf8_lossy(&t.snapshot(Format::Plain).unwrap()).into_owned()
    }

    fn wraps(t: &Terminal) -> Vec<bool> {
        (0..t.total_rows() as u32).map(|y| t.row_flags(y).map(|f| f.0).unwrap_or(false)).collect()
    }

    fn link_at(t: &Terminal, x: u16, y: u32) -> String {
        t.screen_cell(x, y).map(|c| String::from_utf8_lossy(&Terminal::uri(&c)).into_owned()).unwrap_or_default()
    }

    /// A shell's history, longer than the screen: every row comes back, the
    /// long lines still one line, the links still links, the colours still on.
    #[test]
    fn the_history_comes_back_whole() {
        let mut t = mirror(20, 5);
        for n in 0..40 {
            t.write(format!("row {n}\r\n").as_bytes());
        }
        t.write(b"a line long enough to wrap twice over the margin\r\n");
        t.write(b"see \x1b]8;;file:///tmp/a.txt\x1b\\a.txt\x1b]8;;\x1b\\ and \x1b]8;;https://x.y/\x1b\\site\x1b]8;;\x1b\\\r\n");
        t.write(b"\x1b[31mred\x1b[0m plain\r\n");
        for n in 0..10 {
            t.write(format!("later {n}\r\n").as_bytes());
        }
        t.write(b"$ ");
        assert!(t.scrollback_rows() > 40, "the test needs history: {}", t.scrollback_rows());
        let fresh = handed(&t, None);
        assert_eq!(plain(&fresh), plain(&t), "the text, history included");
        assert_eq!(fresh.total_rows(), t.total_rows(), "as many rows");
        assert_eq!(wraps(&fresh), wraps(&t), "every soft wrap where it was");
        let linked = (0..t.total_rows() as u32)
            .find(|&y| link_at(&t, 4, y) == "file:///tmp/a.txt")
            .expect("the link row");
        assert!(linked < t.scrollback_rows() as u32, "the link is in the history");
        assert_eq!(link_at(&fresh, 4, linked), "file:///tmp/a.txt");
        assert_eq!(link_at(&fresh, 14, linked), "https://x.y/");
        assert_eq!(link_at(&fresh, 10, linked), "", "the gap between them is no link");
        assert_eq!(fresh.cursor(), t.cursor());
        let red = |t: &Terminal| {
            String::from_utf8_lossy(&t.snapshot(Format::Vt).unwrap()).contains("\x1b[38;5;1mred")
        };
        assert!(red(&fresh), "the colours come back");
    }

    /// A program on the alternate screen, over a shell with history: its
    /// screen now, and the shell's screen and history once it leaves.
    #[test]
    fn the_main_screen_under_a_program_comes_back_when_it_leaves() {
        let mut t = mirror(30, 6);
        for n in 0..25 {
            t.write(format!("shell output {n}\r\n").as_bytes());
        }
        t.write(b"$ claude\r\n");
        let before = plain(&t);
        let cursor_before = t.cursor();
        t.write(b"\x1b[?1049h\x1b[H\x1b[2J\x1b[2;3HClaude Code\x1b[5;1H> prompt\x1b[>1u\x1b[?2004h");
        let on_alternate = plain(&t);
        let mut fresh = handed(&t, None);
        assert_eq!(fresh.alternate_screen(), Some(1049), "back on the alternate screen");
        assert_eq!(plain(&fresh), on_alternate, "the program's screen");
        assert_eq!(fresh.kitty_keyboard_flags(), 1, "the program's keyboard");
        assert!(fresh.dec_mode(2004), "the program's modes");
        // The program leaves, in the old terminal and in the new one.
        t.write(b"\x1b[?2004l\x1b[<u\x1b[?1049l");
        fresh.write(b"\x1b[?2004l\x1b[<u\x1b[?1049l");
        assert_eq!(fresh.alternate_screen(), None);
        assert_eq!(plain(&fresh), plain(&t), "the shell's screen and history");
        assert_eq!(plain(&t), before, "which is what was there before the program");
        assert_eq!(fresh.cursor(), t.cursor(), "and the cursor where the program found it");
        assert_eq!(t.cursor(), cursor_before);
        assert!(fresh.scrollback_rows() > 15, "with its history: {}", fresh.scrollback_rows());
    }

    /// What a program set about the terminal goes with it.
    #[test]
    fn the_modes_the_title_the_colours_and_the_cursor_go_too() {
        let mut t = mirror(40, 8);
        t.write(b"\x1b]2;working\x07\x1b]11;rgb:01/02/03\x1b\\\x1b]4;5;rgb:aa/bb/cc\x1b\\\x1b[5 q");
        t.write(b"\x1b[?1000h\x1b[?1006h\x1b[?2004h\x1b[?1h\x1b[?25l");
        t.write(b"one\r\ntwo\r\nthree");
        t.write(b"\x1b[?7l");
        let fresh = handed(&t, None);
        assert_eq!(fresh.title(), "working");
        for mode in [1000u16, 1006, 2004, 1] {
            assert!(fresh.dec_mode(mode), "mode {mode} lost");
        }
        assert!(!fresh.dec_mode(25), "the cursor was hidden");
        assert!(!fresh.dec_mode(7), "autowrap was off");
        assert_eq!(fresh.colour(ffi::DATA_COLOR_BACKGROUND), Some(ffi::ColorRgb { r: 1, g: 2, b: 3 }));
        assert_eq!(fresh.palette(ffi::DATA_COLOR_PALETTE).unwrap()[5], ffi::ColorRgb { r: 0xaa, g: 0xbb, b: 0xcc });
        assert_eq!(
            fresh.palette(ffi::DATA_COLOR_PALETTE).unwrap()[6],
            t.palette(ffi::DATA_COLOR_PALETTE_DEFAULT).unwrap()[6],
            "an unchanged colour is left alone"
        );
        assert_eq!(fresh.cursor_shape(), b"\x1b[5 q".to_vec(), "a blinking bar");
        assert_eq!(plain(&fresh), plain(&t));
        // A terminal whose program asked for nothing says nothing about it.
        let mut quiet = mirror(10, 3);
        quiet.write(b"x");
        let bytes = String::from_utf8_lossy(&quiet.handover(None)).into_owned();
        assert!(!bytes.contains(" q"), "no cursor shape: {bytes:?}");
        assert!(!bytes.contains("\x1b]4;"), "no palette: {bytes:?}");
        assert!(!bytes.contains("\x1b]2;"), "no title: {bytes:?}");
    }

    /// Output that ended with a line feed leaves the cursor on a blank row
    /// under it: the blank row comes back, and the next output goes on it,
    /// not over the line above.
    #[test]
    fn the_blank_rows_at_the_bottom_come_back() {
        for lines in [3, 50] {
            let mut t = mirror(20, 8);
            for n in 0..lines {
                t.write(format!("line {n}\r\n").as_bytes());
            }
            let mut fresh = handed(&t, None);
            assert_eq!(fresh.total_rows(), t.total_rows(), "{lines} lines: as many rows");
            assert_eq!(fresh.cursor(), t.cursor(), "{lines} lines: the cursor on the blank row");
            t.write(b"$ next");
            fresh.write(b"$ next");
            assert_eq!(plain(&fresh), plain(&t), "{lines} lines: the next output where it went");
        }
    }

    /// A wide line written with autowrap off overwrites the margin; written
    /// back, it must too, and lines that wrapped before it still wrap.
    #[test]
    fn rows_are_written_before_autowrap_goes_off() {
        let mut t = mirror(10, 4);
        t.write(b"0123456789abc\r\n");
        t.write(b"\x1b[?7lXXXXXXXXXXYZ");
        let fresh = handed(&t, None);
        assert_eq!(plain(&fresh), plain(&t));
        assert_eq!(wraps(&fresh), wraps(&t));
    }

    /// Kitty flags pushed twice on the main screen: popping one after the
    /// handover leaves the first push, not nothing.
    #[test]
    fn the_keyboard_stack_is_kept() {
        let mut t = mirror(20, 4);
        t.write(b"\x1b[>1u\x1b[>5u$ ");
        let keyboard = Keyboard { main: vec![0, 1, 5], alternate: vec![0] };
        let mut fresh = handed(&t, Some(&keyboard));
        assert_eq!(fresh.kitty_keyboard_flags(), 5);
        fresh.write(b"\x1b[<u");
        t.write(b"\x1b[<u");
        assert_eq!(fresh.kitty_keyboard_flags(), t.kitty_keyboard_flags());
        assert_eq!(fresh.kitty_keyboard_flags(), 1);
        // A stack that disagrees with the terminal is not trusted.
        let mut other = mirror(20, 4);
        other.write(b"\x1b[>3u");
        let wrong = Keyboard { main: vec![0, 7], alternate: vec![0] };
        assert_eq!(handed(&other, Some(&wrong)).kitty_keyboard_flags(), 3);
    }

    /// Text whose width depends on grapheme clustering lands in the same
    /// cells.
    #[test]
    fn clusters_land_in_the_same_cells() {
        for clustering in [true, false] {
            let mut t = mirror(20, 3);
            t.write(if clustering { b"\x1b[?2027h" } else { b"\x1b[?2027l" });
            t.write("a\u{1F44D}\u{1F3FD}b \u{2764}\u{FE0F}c".as_bytes());
            let fresh = handed(&t, None);
            assert_eq!(fresh.cursor(), t.cursor(), "clustering {clustering}");
            assert_eq!(plain(&fresh), plain(&t));
        }
    }

    /// A mode the replaying terminal starts with differently is said either
    /// way: blinking switched off by a program stays off.
    #[test]
    fn modes_whose_defaults_differ_are_said_either_way() {
        for on in [true, false] {
            let mut t = mirror(20, 3);
            t.write(if on { b"\x1b[?12h" } else { b"\x1b[?12l" });
            let bytes = String::from_utf8_lossy(&t.handover(None)).into_owned();
            assert!(bytes.contains(if on { "\x1b[?12h" } else { "\x1b[?12l" }), "{bytes:?}");
            // A terminal that starts with it set, as a surface does.
            let mut fresh = Terminal::new(20, 3).unwrap();
            fresh.write(b"\x1b[?12h");
            fresh.write(bytes.as_bytes());
            assert_eq!(fresh.dec_mode(12), on);
        }
    }

    /// A program on the alternate screen inside a scrolling region of its
    /// own, origin mode on, autowrap and insert mode as it left them, over a
    /// shell with history: every row of its screen comes back where it was,
    /// the cursor too, and what it writes next lands as it would have.
    #[test]
    fn a_program_in_its_own_scrolling_region_comes_back_whole() {
        let mut t = mirror(12, 8);
        for n in 0..30 {
            t.write(format!("shell {n}\r\n").as_bytes());
        }
        t.write(b"$ tmux");
        let cursor_before = t.cursor();
        t.write(b"\x1b[?1049h\x1b[H\x1b[2J");
        for n in 1..=8 {
            t.write(format!("\x1b[{n};1Hpane row {n}").as_bytes());
        }
        // The region, origin mode, autowrap off, insert mode on, a pen.
        t.write(b"\x1b[2;6r\x1b[?6h\x1b[?7l\x1b[4h\x1b[2;3H\x1b[31m");
        let mut fresh = handed(&t, None);
        assert_eq!(plain(&fresh), plain(&t), "the program's rows, none lost or moved");
        assert_eq!(fresh.cursor(), t.cursor(), "the cursor, inside the region");
        for mode in [6u16, 7] {
            assert_eq!(fresh.dec_mode(mode), t.dec_mode(mode), "mode {mode}");
        }
        for bytes in [&b"XY"[..], b"\x1b[H>", b"\x1b[9;1Hbottom\r\n\r\n\r\n\r\n\r\n"] {
            t.write(bytes);
            fresh.write(bytes);
            assert_eq!(plain(&fresh), plain(&t), "after {:?}", String::from_utf8_lossy(bytes));
            assert_eq!(fresh.cursor(), t.cursor(), "after {:?}", String::from_utf8_lossy(bytes));
        }
        assert_eq!(vt(&fresh), vt(&t), "the pen it writes with");
        let leave = b"\x1b[4l\x1b[?7h\x1b[?6l\x1b[r\x1b[?1049l";
        t.write(leave);
        fresh.write(leave);
        assert_eq!(plain(&fresh), plain(&t), "the shell under it");
        assert_eq!(fresh.cursor(), cursor_before, "and its cursor");
    }

    /// Origin mode on the main screen: the cursor is put back where it was,
    /// not as far below the top margin as it was below the top of the screen.
    #[test]
    fn origin_mode_puts_the_cursor_back_within_the_margins() {
        let mut t = mirror(20, 8);
        t.write(b"line1\r\nline2\r\nline3\r\n\x1b[3;7r\x1b[?6h\x1b[2;5H");
        let mut fresh = handed(&t, None);
        assert_eq!(fresh.cursor(), t.cursor());
        t.write(b"X\x1b[1;1HY");
        fresh.write(b"X\x1b[1;1HY");
        assert_eq!(plain(&fresh), plain(&t));
        // And with left and right margins as well.
        let mut t = mirror(20, 8);
        t.write(b"abc\x1b[?69h\x1b[2;7r\x1b[4;15s\x1b[?6h\x1b[3;6H");
        let mut fresh = handed(&t, None);
        assert_eq!(fresh.cursor(), t.cursor());
        t.write(b"Z\x1b[1;1HW");
        fresh.write(b"Z\x1b[1;1HW");
        assert_eq!(plain(&fresh), plain(&t));
    }

    /// A row that wrapped, its continuation erased since: the next row
    /// stays where it is. And a wrapped row erased whole by a selective
    /// erase, which keeps the wrap: the next row's text does not move up
    /// into it.
    #[test]
    fn a_wrap_whose_continuation_was_erased_moves_nothing() {
        for history in [0, 30] {
            let mut t = mirror(10, 8);
            for n in 0..history {
                t.write(format!("old {n}\r\n").as_bytes());
            }
            t.write(b"0123456789abc\x1b[2K\r\nnext\r\n");
            t.write(b"AAAA\r\n0123456789abc\x1b[2A\x1b[?J\x1b[2B\x1b[1GX\r\n");
            t.write(b"ABCDEFGHIJklm\x1b[1A\x1b[6G\x1b[K\x1b[2Bafter");
            let mut fresh = handed(&t, None);
            assert_eq!(fresh.total_rows(), t.total_rows(), "{history}: as many rows");
            assert_eq!(plain(&fresh), plain(&t), "{history}: every row where it was");
            assert_eq!(fresh.cursor(), t.cursor(), "{history}: the cursor");
            t.write(b"\r\n$ ");
            fresh.write(b"\r\n$ ");
            assert_eq!(plain(&fresh), plain(&t), "{history}: the next output where it goes");
        }
    }

    /// What ESC 7 saved comes back with ESC 8 after the handover: the place,
    /// the pen — on the main screen, on the alternate one, and on the main
    /// one under it when the program went there by 47.
    #[test]
    fn the_cursor_saved_by_esc_7_comes_back() {
        let mut t = mirror(20, 6);
        t.write(b"one\r\ntwo\r\nthree\x1b[2;5H\x1b[31m\x1b7\x1b[6;1H\x1b[0m$ ");
        let mut fresh = handed(&t, None);
        assert_eq!(fresh.cursor(), t.cursor(), "the cursor in use");
        assert_eq!(vt(&fresh), vt(&t), "and its pen");
        t.write(b"\x1b8X");
        fresh.write(b"\x1b8X");
        assert_eq!(plain(&fresh), plain(&t), "ESC 8 goes back to where ESC 7 was");
        assert_eq!(vt(&fresh), vt(&t), "with the pen it had");

        for (mode, leave) in [(1049u16, &b"\x1b[?1049l"[..]), (47, b"\x1b[?47l")] {
            let mut t = mirror(20, 6);
            t.write(b"shell\r\n\x1b[3;7H\x1b7\x1b[1;1H");
            t.write(format!("\x1b[?{mode}h\x1b[H\x1b[2Jprogram\x1b[4;9H\x1b[1m\x1b7\x1b[1;1H\x1b[0m").as_bytes());
            let mut fresh = handed(&t, None);
            assert_eq!(fresh.cursor(), t.cursor(), "{mode}: the cursor in use");
            t.write(b"\x1b8Y");
            fresh.write(b"\x1b8Y");
            assert_eq!(plain(&fresh), plain(&t), "{mode}: ESC 8 on the alternate screen");
            assert_eq!(vt(&fresh), vt(&t), "{mode}: with its pen");
            t.write(leave);
            fresh.write(leave);
            assert_eq!(fresh.cursor(), t.cursor(), "{mode}: the main screen's cursor once it leaves");
            t.write(b"\x1b8Z");
            fresh.write(b"\x1b8Z");
            assert_eq!(plain(&fresh), plain(&t), "{mode}: ESC 8 on the main screen");
        }
    }

    /// Output that the handover cut in the middle of a sequence: the rest of
    /// it, arriving afterwards, finishes it there as it would have here.
    #[test]
    fn a_sequence_cut_in_two_by_the_handover_is_finished_there() {
        let cases: [(&[u8], &[u8]); 5] = [
            (b"hello \x1b[38;5;", b"196mX\x1b[0m done"),
            ("caf\u{e9}".as_bytes().split_at(4).0, b"\xa9!"),
            (b"\x1b]2;tit", b"le\x07ok"),
            (b"see \x1b]8;;https://exa", b"mple.com/\x1b\\link\x1b]8;;\x1b\\ after"),
            (b"\x1b[?1049h\x1b[2;2Hon the alternate screen \x1b[1", b";31mred"),
        ];
        for (before, after) in cases {
            let mut t = mirror(30, 5);
            t.write(b"$ ");
            t.write(before);
            let mut fresh = handed(&t, None);
            t.write(after);
            fresh.write(after);
            let what = String::from_utf8_lossy(before).into_owned();
            assert_eq!(plain(&fresh), plain(&t), "{what:?}");
            assert_eq!(vt(&fresh), vt(&t), "{what:?}");
            assert_eq!(fresh.title(), t.title(), "{what:?}");
            let linked = |t: &Terminal| (0..30).map(|x| link_at(t, x, 0)).collect::<Vec<_>>();
            assert_eq!(linked(&fresh), linked(&t), "{what:?}");
        }
    }

    #[test]
    fn the_helpers_read_the_formatter_right() {
        assert_eq!(split_rows(b"a\r\nb\r\n\r\nc"), vec![&b"a"[..], b"b", b"", b"c"]);
        assert_eq!(leading_modes(b"\x1b[?1h\x1b[?7l\x1b[3g\x1b[1G\x1bH\x1b[Hhello"), 23);
        assert_eq!(leading_modes(b"\x1b[0mhi"), 0, "a pen is not a mode");
        assert_eq!(without_screen_switches(b"\x1b[?1h\x1b[?1049h\x1b[?47l\x1b[?2004h"), b"\x1b[?1h\x1b[?2004h");
        assert_eq!(visible(b"\x1b[0m\x1b]8;;u\x1b\\ab\x1b]8;;\x07c  "), b"abc");
    }
}
