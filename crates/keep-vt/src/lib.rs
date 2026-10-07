//! Safe bindings for `libghostty-vt`: terminal state without rendering.
//!
//! This crate owns the piece that makes detach/attach possible. A `Terminal`
//! consumes the raw byte stream coming out of a PTY, keeps the resulting
//! screen state, and can replay that state later — which is exactly what a
//! client needs when it reconnects to a session it did not start.
//!
//! The library does no rendering, no font handling and no PTY management.

use std::ffi::c_void;
use std::os::raw::c_int;

/// Raw FFI surface. Prefer the safe wrappers below.
pub mod ffi {
    use std::ffi::c_void;
    use std::os::raw::c_int;

    #[repr(C)]
    pub struct TerminalImpl {
        _private: [u8; 0],
    }
    pub type Terminal = *mut TerminalImpl;

    #[repr(C)]
    pub struct FormatterImpl {
        _private: [u8; 0],
    }
    pub type Formatter = *mut FormatterImpl;

    pub const SUCCESS: c_int = 0;

    /// `GhosttyTerminalOption` values we set.
    pub const OPT_USERDATA: c_int = 0;
    pub const OPT_WRITE_PTY: c_int = 1;

    /// What the terminal calls with the bytes it would write back to the
    /// PTY: the answers to the questions a program asks its terminal.
    pub type WritePtyFn =
        unsafe extern "C" fn(terminal: Terminal, userdata: *mut c_void, data: *const u8, len: usize);

    /// `GhosttyTerminalData` values we read.
    pub const DATA_COLS: c_int = 1;
    pub const DATA_ROWS: c_int = 2;
    pub const DATA_CURSOR_X: c_int = 3;
    pub const DATA_CURSOR_Y: c_int = 4;
    pub const DATA_KITTY_KEYBOARD_FLAGS: c_int = 8;
    pub const DATA_TITLE: c_int = 12;
    pub const DATA_PWD: c_int = 13;
    pub const DATA_MODE: c_int = 37;

    /// `GhosttyGridRef`: a cell, resolved.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct GridRef {
        pub size: usize,
        pub node: *mut c_void,
        pub x: u16,
        pub y: u16,
    }

    /// `GhosttyPoint` with the `active` tag: a cell of the screen the
    /// program draws on, counted from its top left.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct PointCoordinate {
        pub x: u16,
        pub y: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union PointValue {
        pub coordinate: PointCoordinate,
        pub padding: [u64; 2],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Point {
        pub tag: c_int,
        pub value: PointValue,
    }

    pub const POINT_TAG_ACTIVE: c_int = 0;

    /// `GhosttySelection`: the cells from `start` to `end`, both included.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Selection {
        pub size: usize,
        pub start: GridRef,
        pub end: GridRef,
        pub rectangle: bool,
    }

    pub type Row = u64;
    /// `GhosttyRowData`: whether any cell of the row may have a hyperlink.
    pub const ROW_DATA_HYPERLINK: c_int = 5;
    pub const OUT_OF_SPACE: c_int = -3;

    /// `GhosttyTerminalModeConfig`: a mode to ask about, and its answer.
    /// `mode` is a `GhosttyMode` — the number, with bit 15 set for ANSI
    /// modes and clear for DEC private ones.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ModeConfig {
        pub mode: u16,
        pub value: bool,
    }

    /// A borrowed string owned by the terminal. Only valid until the next
    /// mutating call, so copy before releasing the lock.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct GhosttyString {
        pub ptr: *const u8,
        pub len: usize,
    }

    pub const FORMAT_PLAIN: c_int = 0;
    pub const FORMAT_VT: c_int = 1;
    pub const FORMAT_HTML: c_int = 2;

    /// Screen-level extras. `size` must be set to the struct size.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ScreenExtra {
        pub size: usize,
        pub cursor: bool,
        pub style: bool,
        pub hyperlink: bool,
        pub protection: bool,
        pub kitty_keyboard: bool,
        pub charsets: bool,
    }

    /// Terminal-level extras. `size` must be set to the struct size.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct TerminalExtra {
        pub size: usize,
        pub palette: bool,
        pub modes: bool,
        pub scrolling_region: bool,
        pub tabstops: bool,
        pub pwd: bool,
        pub keyboard: bool,
        pub screen: ScreenExtra,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct FormatterOptions {
        /// Must be set to the struct size; this is a versioned struct.
        pub size: usize,
        pub emit: c_int,
        pub unwrap: bool,
        pub trim: bool,
        pub extra: TerminalExtra,
        pub selection: *const c_void,
    }

    extern "C" {
        pub fn ghostty_terminal_new(
            allocator: *const c_void,
            out: *mut Terminal,
            cols: u16,
            rows: u16,
        ) -> c_int;
        pub fn ghostty_terminal_free(terminal: Terminal);
        pub fn ghostty_terminal_get(terminal: Terminal, data: c_int, out: *mut c_void) -> c_int;
        pub fn ghostty_terminal_set(terminal: Terminal, option: c_int, value: *const c_void) -> c_int;
        pub fn ghostty_terminal_vt_write(terminal: Terminal, data: *const u8, len: usize);
        pub fn ghostty_terminal_resize(
            terminal: Terminal,
            cols: u16,
            rows: u16,
            cell_width_px: u32,
            cell_height_px: u32,
        ) -> c_int;

        pub fn ghostty_formatter_terminal_new(
            allocator: *const c_void,
            out: *mut Formatter,
            terminal: Terminal,
            options: FormatterOptions,
        ) -> c_int;
        pub fn ghostty_formatter_format_alloc(
            formatter: Formatter,
            allocator: *const c_void,
            out_ptr: *mut *mut u8,
            out_len: *mut usize,
        ) -> c_int;
        pub fn ghostty_formatter_free(formatter: Formatter);

        pub fn ghostty_terminal_grid_ref(terminal: Terminal, point: Point, out: *mut GridRef) -> c_int;
        pub fn ghostty_grid_ref_row(grid_ref: *const GridRef, out: *mut Row) -> c_int;
        pub fn ghostty_row_get(row: Row, data: c_int, out: *mut c_void) -> c_int;
        pub fn ghostty_grid_ref_hyperlink_uri(
            grid_ref: *const GridRef,
            buf: *mut u8,
            buf_len: usize,
            out_len: *mut usize,
        ) -> c_int;

        pub fn ghostty_free(allocator: *const c_void, ptr: *mut u8, len: usize);
    }
}

/// What a formatted snapshot should look like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Plain text, no escape sequences. Good for search and previews.
    Plain,
    /// VT sequences preserving colors, styles and hyperlinks.
    ///
    /// This is the repaint format: write it to a host terminal and the
    /// screen is restored as the session left it.
    Vt,
    /// HTML with inline styles.
    Html,
}

impl Format {
    fn as_raw(self) -> c_int {
        match self {
            Format::Plain => ffi::FORMAT_PLAIN,
            Format::Vt => ffi::FORMAT_VT,
            Format::Html => ffi::FORMAT_HTML,
        }
    }
}

/// A libghostty-vt error code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error(pub c_int);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "libghostty-vt error {}", self.0)
    }
}

impl std::error::Error for Error {}

/// A headless terminal: feed it PTY bytes, ask it for the screen.
pub struct Terminal {
    raw: ffi::Terminal,
    /// The answers the terminal has written, waiting to be collected; see
    /// [`Terminal::answer_queries`]. Boxed so the address the library holds
    /// stays put when the terminal moves.
    replies: Option<Box<Vec<u8>>>,
}

/// How much unanswered reply is kept. A program asking the same question in
/// a loop with nobody collecting must not grow this without end.
const REPLY_LIMIT: usize = 64 * 1024;

unsafe extern "C" fn collect_reply(
    _terminal: ffi::Terminal,
    userdata: *mut c_void,
    data: *const u8,
    len: usize,
) {
    if userdata.is_null() || data.is_null() || len == 0 {
        return;
    }
    let buf = unsafe { &mut *(userdata as *mut Vec<u8>) };
    if buf.len() + len <= REPLY_LIMIT {
        buf.extend_from_slice(unsafe { std::slice::from_raw_parts(data, len) });
    }
}

// The handle is owned exclusively; libghostty-vt forbids concurrent writes.
unsafe impl Send for Terminal {}

impl Terminal {
    pub fn new(cols: u16, rows: u16) -> Result<Self, Error> {
        let mut raw: ffi::Terminal = std::ptr::null_mut();
        let rc = unsafe { ffi::ghostty_terminal_new(std::ptr::null(), &mut raw, cols, rows) };
        if rc != ffi::SUCCESS {
            return Err(Error(rc));
        }
        Ok(Self { raw, replies: None })
    }

    /// Have the terminal answer what programs ask of it — where the cursor
    /// is, which modes are set — into a buffer, collected with
    /// [`Terminal::take_replies`].
    ///
    /// By default the questions go unanswered here, because whoever is
    /// watching the tab answers them; this is for when nobody is.
    pub fn answer_queries(&mut self) -> Result<(), Error> {
        if self.replies.is_some() {
            return Ok(());
        }
        let mut buf: Box<Vec<u8>> = Box::default();
        let userdata = &mut *buf as *mut Vec<u8> as *const c_void;
        let callback: ffi::WritePtyFn = collect_reply;
        let rc = unsafe { ffi::ghostty_terminal_set(self.raw, ffi::OPT_USERDATA, userdata) };
        if rc != ffi::SUCCESS {
            return Err(Error(rc));
        }
        let rc = unsafe {
            ffi::ghostty_terminal_set(self.raw, ffi::OPT_WRITE_PTY, callback as *const c_void)
        };
        if rc != ffi::SUCCESS {
            return Err(Error(rc));
        }
        self.replies = Some(buf);
        Ok(())
    }

    /// The answers written since the last call, oldest first.
    pub fn take_replies(&mut self) -> Vec<u8> {
        self.replies.as_mut().map(|b| std::mem::take(&mut **b)).unwrap_or_default()
    }

    /// Feed bytes read from the PTY.
    pub fn write(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        unsafe { ffi::ghostty_terminal_vt_write(self.raw, data.as_ptr(), data.len()) };
    }

    /// The title the program inside set with OSC 0/2.
    ///
    /// Empty when nothing has set one. Shells and editors set this constantly,
    /// which makes it the cheapest way to label a tab with what it is doing.
    pub fn title(&self) -> String {
        self.borrowed_string(ffi::DATA_TITLE)
    }

    /// The working directory the program inside reported via OSC 7.
    pub fn pwd(&self) -> String {
        self.borrowed_string(ffi::DATA_PWD)
    }

    /// The Kitty keyboard protocol flags the program inside has asked for,
    /// on the screen it is using. Zero when it never asked.
    pub fn kitty_keyboard_flags(&self) -> u8 {
        let mut flags: u8 = 0;
        let rc = unsafe {
            ffi::ghostty_terminal_get(
                self.raw,
                ffi::DATA_KITTY_KEYBOARD_FLAGS,
                &mut flags as *mut u8 as *mut c_void,
            )
        };
        if rc == ffi::SUCCESS { flags } else { 0 }
    }

    /// Whether a DEC private mode (`CSI ? n h`) is set.
    pub fn dec_mode(&self, number: u16) -> bool {
        let mut config = ffi::ModeConfig { mode: number & 0x7FFF, value: false };
        let rc = unsafe {
            ffi::ghostty_terminal_get(
                self.raw,
                ffi::DATA_MODE,
                &mut config as *mut ffi::ModeConfig as *mut c_void,
            )
        };
        rc == ffi::SUCCESS && config.value
    }

    fn borrowed_string(&self, data: c_int) -> String {
        let mut out = ffi::GhosttyString { ptr: std::ptr::null(), len: 0 };
        let rc = unsafe {
            ffi::ghostty_terminal_get(self.raw, data, &mut out as *mut _ as *mut c_void)
        };
        if rc != ffi::SUCCESS || out.ptr.is_null() || out.len == 0 {
            return String::new();
        }
        // Copy immediately: the pointer dies on the next mutating call.
        let bytes = unsafe { std::slice::from_raw_parts(out.ptr, out.len) };
        String::from_utf8_lossy(bytes).into_owned()
    }

    /// Resize the grid.
    ///
    /// Must be kept in step with the PTY window size, or the child will draw
    /// for one geometry while the grid tracks another.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), Error> {
        let rc = unsafe { ffi::ghostty_terminal_resize(self.raw, cols, rows, 0, 0) };
        if rc != ffi::SUCCESS {
            return Err(Error(rc));
        }
        Ok(())
    }

    /// Render the current screen state into the requested format.
    ///
    /// The VT form is what an attaching client is painted with, so it carries
    /// what the program switched on as well as what it drew — and says so in
    /// an order a terminal replaying it will not get wrong:
    ///
    /// - The alternate screen first, when the program is on it. Switching
    ///   saves the cursor with the modes of the moment, and a switch replayed
    ///   after the program's modes saved those instead — origin mode among
    ///   them, which then came back on when the program left.
    /// - The Kitty keyboard flags last, and always, zero included. A repaint
    ///   that said nothing when the program asked for nothing left a terminal
    ///   that had missed the program's pop with the flags still on, and a
    ///   shell reading keys it never asked for.
    pub fn snapshot(&self, format: Format) -> Result<Vec<u8>, Error> {
        let body = self.formatted(format)?;
        if format != Format::Vt {
            return Ok(body);
        }
        let mut out = Vec::with_capacity(body.len() + 160);
        // The screen, said either way, and cleared once it is the one being
        // painted: a terminal that missed the program leaving its alternate
        // screen is taken back to the main one, not painted over where it is.
        match [1049u16, 1047, 47].into_iter().find(|&m| self.dec_mode(m)) {
            Some(1049) => out.extend_from_slice(b"\x1b[?1049h"),
            Some(mode) => out.extend_from_slice(format!("\x1b[?{mode}h\x1b[H\x1b[2J").as_bytes()),
            None => out.extend_from_slice(b"\x1b[?1049l\x1b[H\x1b[2J"),
        }
        // The modes that change what the keyboard and mouse send, switched off
        // where the program has them off — the body below only ever switches
        // on, so a terminal that missed the program switching one off would
        // otherwise keep it. modifyOtherKeys likewise: off here, back on in
        // the body if the program has it.
        for mode in Self::INPUT_MODES {
            if !self.dec_mode(mode) {
                out.extend_from_slice(format!("\x1b[?{mode}l").as_bytes());
            }
        }
        out.extend_from_slice(b"\x1b[>4m");
        // The body leaves hyperlinks out: its OSC 8 is only the one the
        // cursor is inside of. Each linked run is written again over itself,
        // inside its link, before the body's last words put the cursor and
        // its style back where the program had them.
        let links = self.hyperlinks();
        let cursor = format!("\x1b[{};{}H", self.cursor().1 + 1, self.cursor().0 + 1);
        let at = (!links.is_empty())
            .then(|| body.windows(cursor.len()).rposition(|w| w == cursor.as_bytes()))
            .flatten();
        match at {
            Some(at) => {
                out.extend_from_slice(&body[..at]);
                out.extend_from_slice(&links);
                out.extend_from_slice(&body[at..]);
            }
            None => out.extend_from_slice(&body),
        }
        out.extend_from_slice(format!("\x1b[={};1u", self.kitty_keyboard_flags()).as_bytes());
        Ok(out)
    }

    fn get_u16(&self, data: c_int) -> u16 {
        let mut out: u16 = 0;
        let rc = unsafe { ffi::ghostty_terminal_get(self.raw, data, &mut out as *mut u16 as *mut c_void) };
        if rc == ffi::SUCCESS { out } else { 0 }
    }

    /// Column and row of the cursor on the active screen, from zero.
    fn cursor(&self) -> (u16, u16) {
        (self.get_u16(ffi::DATA_CURSOR_X), self.get_u16(ffi::DATA_CURSOR_Y))
    }

    fn cell(&self, x: u16, y: u16) -> Option<ffi::GridRef> {
        let point = ffi::Point {
            tag: ffi::POINT_TAG_ACTIVE,
            value: ffi::PointValue { coordinate: ffi::PointCoordinate { x, y: y as u32 } },
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

    fn uri(cell: &ffi::GridRef) -> Vec<u8> {
        let mut buf = vec![0u8; 256];
        let mut len: usize = 0;
        let mut rc = unsafe { ffi::ghostty_grid_ref_hyperlink_uri(cell, buf.as_mut_ptr(), buf.len(), &mut len) };
        if rc == ffi::OUT_OF_SPACE {
            buf = vec![0u8; len];
            rc = unsafe { ffi::ghostty_grid_ref_hyperlink_uri(cell, buf.as_mut_ptr(), buf.len(), &mut len) };
        }
        if rc != ffi::SUCCESS {
            return Vec::new();
        }
        buf.truncate(len);
        buf
    }

    /// Every run of cells on the active screen that shares a hyperlink,
    /// written as it is on screen, styles and all, inside its OSC 8 and at
    /// its place. Empty when there is none.
    ///
    /// Claude Code links what it prints (a markdown link to a file becomes
    /// `file://…`), and a client attaching to the tab after it printed would
    /// otherwise get the text without the link, which a ⌘-click then cannot
    /// open.
    fn hyperlinks(&self) -> Vec<u8> {
        let (cols, rows) = (self.get_u16(ffi::DATA_COLS), self.get_u16(ffi::DATA_ROWS));
        let mut out = Vec::new();
        for y in 0..rows {
            let Some(first) = self.cell(0, y) else { continue };
            let mut row: ffi::Row = 0;
            let mut linked = false;
            let has = unsafe {
                ffi::ghostty_grid_ref_row(&first, &mut row) == ffi::SUCCESS
                    && ffi::ghostty_row_get(row, ffi::ROW_DATA_HYPERLINK, &mut linked as *mut bool as *mut c_void)
                        == ffi::SUCCESS
            };
            if !has || !linked {
                continue;
            }
            let mut x = 0;
            while x < cols {
                let Some(start) = self.cell(x, y) else { break };
                let uri = Self::uri(&start);
                if uri.is_empty() {
                    x += 1;
                    continue;
                }
                let mut last = start;
                let mut end = x;
                while end + 1 < cols {
                    let Some(next) = self.cell(end + 1, y) else { break };
                    if Self::uri(&next) != uri {
                        break;
                    }
                    last = next;
                    end += 1;
                }
                if let Ok(text) = self.formatted_run(start, last) {
                    out.extend_from_slice(format!("\x1b[{};{}H\x1b[0m\x1b]8;;", y + 1, x + 1).as_bytes());
                    out.extend_from_slice(&uri);
                    out.extend_from_slice(b"\x1b\\");
                    out.extend_from_slice(&text);
                    out.extend_from_slice(b"\x1b]8;;\x1b\\");
                }
                x = end + 1;
            }
        }
        out
    }

    /// The cells from `start` to `end` as VT: their text and its styles,
    /// nothing about the terminal around them.
    fn formatted_run(&self, start: ffi::GridRef, end: ffi::GridRef) -> Result<Vec<u8>, Error> {
        let selection = ffi::Selection {
            size: std::mem::size_of::<ffi::Selection>(),
            start,
            end,
            rectangle: false,
        };
        let none = ffi::ScreenExtra {
            size: std::mem::size_of::<ffi::ScreenExtra>(),
            cursor: false,
            style: false,
            hyperlink: false,
            protection: false,
            kitty_keyboard: false,
            charsets: false,
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
                screen: none,
            },
            selection: &selection as *const ffi::Selection as *const c_void,
        };
        self.format_with(options)
    }

    /// DEC modes that decide what a key press or a mouse movement writes:
    /// application cursor keys, mouse reporting in all its encodings, focus
    /// reports, bracketed paste.
    const INPUT_MODES: [u16; 11] = [1, 9, 1000, 1002, 1003, 1005, 1006, 1015, 1016, 1004, 2004];

    fn formatted(&self, format: Format) -> Result<Vec<u8>, Error> {
        let extra_screen = ffi::ScreenExtra {
            size: std::mem::size_of::<ffi::ScreenExtra>(),
            cursor: true,
            style: true,
            hyperlink: true,
            protection: false,
            // Written by `snapshot` itself, so that it is said even when it
            // is zero.
            kitty_keyboard: false,
            charsets: false,
        };
        let options = ffi::FormatterOptions {
            size: std::mem::size_of::<ffi::FormatterOptions>(),
            emit: format.as_raw(),
            unwrap: false,
            trim: false,
            extra: ffi::TerminalExtra {
                size: std::mem::size_of::<ffi::TerminalExtra>(),
                palette: false,
                // Bracketed paste, mouse reporting, the alternate screen,
                // application cursor keys: whatever the program switched on is
                // switched on again for whoever attaches.
                modes: true,
                scrolling_region: false,
                tabstops: false,
                // The directory the shell announced. A client that attaches
                // learns it from the snapshot rather than waiting for the next
                // prompt, which is what decides where a new tab opens.
                pwd: true,
                // modifyOtherKeys, for the same reason as the Kitty flags.
                keyboard: true,
                screen: extra_screen,
            },
            selection: std::ptr::null(),
        };
        self.format_with(options)
    }

    fn format_with(&self, options: ffi::FormatterOptions) -> Result<Vec<u8>, Error> {
        let mut formatter: ffi::Formatter = std::ptr::null_mut();
        let rc = unsafe {
            ffi::ghostty_formatter_terminal_new(
                std::ptr::null(),
                &mut formatter,
                self.raw,
                options,
            )
        };
        if rc != ffi::SUCCESS {
            return Err(Error(rc));
        }

        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut len: usize = 0;
        let rc =
            unsafe { ffi::ghostty_formatter_format_alloc(formatter, std::ptr::null(), &mut ptr, &mut len) };
        if rc != ffi::SUCCESS {
            unsafe { ffi::ghostty_formatter_free(formatter) };
            return Err(Error(rc));
        }

        // Nothing to say comes back as no allocation at all, and a slice may
        // not be made from a null pointer even when it is empty.
        if ptr.is_null() || len == 0 {
            unsafe { ffi::ghostty_formatter_free(formatter) };
            return Ok(Vec::new());
        }
        let out = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe {
            ffi::ghostty_free(std::ptr::null(), ptr, len);
            ffi::ghostty_formatter_free(formatter);
        }
        Ok(out)
    }

    /// Convenience: the screen as plain text.
    pub fn text(&self) -> Result<String, Error> {
        Ok(String::from_utf8_lossy(&self.snapshot(Format::Plain)?).into_owned())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        unsafe { ffi::ghostty_terminal_free(self.raw) };
    }
}

#[allow(unused)]
fn _assert_c_void_used(_: *const c_void) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_replays_screen_state() {
        let mut t = Terminal::new(60, 8).expect("terminal");
        t.write(b"\x1b[2J\x1b[H");
        t.write(b"\x1b[1;32mproject\x1b[0m: ~/www/777leads\r\n");
        // Cursor pushed past the last row must clamp, keeping the column.
        t.write(b"\x1b[10;5H(clamped)");
        t.write(b"\x1b[3;1Hrow three");

        let text = t.text().expect("text");
        assert!(text.contains("project: ~/www/777leads"), "got: {text:?}");
        assert!(text.contains("row three"), "got: {text:?}");
        assert!(text.contains("    (clamped)"), "column not preserved: {text:?}");

        // The VT snapshot must carry styling the plain one drops.
        let vt = t.snapshot(Format::Vt).expect("vt");
        assert!(vt.windows(2).any(|w| w == b"\x1b["), "no escapes in VT snapshot");
        assert!(vt.len() > text.len(), "VT snapshot should be richer than plain");
    }

    #[test]
    fn snapshot_keeps_the_links_on_screen() {
        // What Claude Code writes for a markdown link to a file: OSC 8 with
        // an id, ended by BEL, the text coloured.
        let mut t = Terminal::new(60, 6).unwrap();
        t.write(b"\x1b[?1049h\x1b[H  veja \x1b]8;id=a1;file:///tmp/n%20a.txt\x07\x1b[94mnota\x1b]8;;\x07\x1b[39m e mais\r\n");
        t.write(b"\x1b[3;5H\x1b]8;;https://example.com/x\x1b\\site\x1b]8;;\x1b\\\x1b[5;9H\x1b[1m");
        let fresh = replayed(&t, 60, 6);
        let vt = String::from_utf8_lossy(&fresh.snapshot(Format::Vt).unwrap()).into_owned();
        assert!(vt.contains("\x1b]8;;file:///tmp/n%20a.txt\x1b\\"), "file link lost: {vt:?}");
        assert!(vt.contains("\x1b]8;;https://example.com/x\x1b\\"), "web link lost: {vt:?}");
        // Text, cursor and pen as the program left them.
        assert_eq!(fresh.text().unwrap(), t.text().unwrap());
        assert_eq!(fresh.cursor(), (8, 4));
        let tail = |t: &Terminal| {
            let vt = String::from_utf8_lossy(&t.snapshot(Format::Vt).unwrap()).into_owned();
            vt[vt.rfind("\x1b[5;9H").expect("no cursor")..].to_string()
        };
        assert_eq!(tail(&fresh), tail(&t), "the cursor and its style are not the last word");
        // A screen without links is said exactly as before.
        let mut plain = Terminal::new(60, 6).unwrap();
        plain.write(b"sem link\r\n");
        assert!(!String::from_utf8_lossy(&plain.snapshot(Format::Vt).unwrap()).contains("]8;;"));
    }

    /// Replay a snapshot into a terminal that has never seen anything else,
    /// which is what a client attaching to a tab is.
    fn replayed(t: &Terminal, cols: u16, rows: u16) -> Terminal {
        let mut fresh = Terminal::new(cols, rows).unwrap();
        fresh.write(&t.snapshot(Format::Vt).unwrap());
        fresh
    }

    #[test]
    fn snapshot_carries_the_keyboard_the_program_asked_for() {
        // What Claude Code writes on start: the Kitty keyboard protocol at
        // flags 5, modifyOtherKeys at level 2, and bracketed paste.
        let mut t = Terminal::new(40, 5).unwrap();
        t.write(b"\x1b[>5u\x1b[>4;2m\x1b[?2004hprompt> ");
        assert_eq!(t.kitty_keyboard_flags(), 5);

        let fresh = replayed(&t, 40, 5);
        assert_eq!(fresh.kitty_keyboard_flags(), 5, "kitty flags lost in the repaint");
        assert!(fresh.dec_mode(2004), "bracketed paste lost in the repaint");
        assert!(fresh.text().unwrap().contains("prompt>"), "screen lost in the repaint");
    }

    #[test]
    fn snapshot_of_a_plain_shell_asks_for_nothing() {
        let mut t = Terminal::new(40, 5).unwrap();
        t.write(b"$ ls\r\nfile\r\n$ ");
        let fresh = replayed(&t, 40, 5);
        assert_eq!(fresh.kitty_keyboard_flags(), 0);
        assert!(!fresh.dec_mode(2004));
        assert!(!fresh.dec_mode(1049));
    }

    /// A terminal that missed the program's pop — output dropped for a slow
    /// client — is told the flags are off, not left to assume.
    #[test]
    fn snapshot_says_the_flags_are_off_when_they_are() {
        let mut t = Terminal::new(40, 5).unwrap();
        t.write(b"\x1b[>5u\x1b[<u$ ");
        let vt = t.snapshot(Format::Vt).unwrap();
        assert!(vt.ends_with(b"\x1b[=0;1u"), "flags not declared: {:?}", String::from_utf8_lossy(&vt));

        let mut stale = Terminal::new(40, 5).unwrap();
        stale.write(b"\x1b[>5u");
        stale.write(&vt);
        assert_eq!(stale.kitty_keyboard_flags(), 0, "stale flags survived the repaint");
    }

    /// A terminal that missed the program switching bracketed paste and
    /// mouse reporting off, or leaving its alternate screen, is set right.
    #[test]
    fn snapshot_turns_off_what_the_program_turned_off() {
        let mut t = Terminal::new(40, 5).unwrap();
        t.write(b"$ ");
        let mut stale = Terminal::new(40, 5).unwrap();
        stale.write(b"\x1b[?1049h\x1b[?2004h\x1b[?1000h\x1b[?1006h\x1b[?1h");
        stale.write(&t.snapshot(Format::Vt).unwrap());
        for mode in [1049, 2004, 1000, 1006, 1] {
            assert!(!stale.dec_mode(mode), "mode {mode} survived the repaint");
        }
        assert!(stale.text().unwrap().contains('$'));
    }

    /// Origin mode on the alternate screen: the replayed switch must save
    /// the cursor before the program's modes are replayed, as the program's
    /// own switch did, or leaving the screen restores them.
    #[test]
    fn leaving_a_replayed_alternate_screen_restores_what_the_program_left() {
        let mut t = Terminal::new(40, 8).unwrap();
        t.write(b"$ prog\r\n\x1b[?1049h\x1b[2;5r\x1b[?6hx");
        let mut outer = replayed(&t, 40, 8);
        let leave = b"\x1b[r\x1b[?6l\x1b[?1049l";
        t.write(leave);
        outer.write(leave);
        assert_eq!(t.dec_mode(6), outer.dec_mode(6), "origin mode came back different");
        assert!(!outer.dec_mode(6));
    }

    #[test]
    fn snapshot_of_the_alternate_screen_lands_on_the_alternate_screen() {
        // An editor: main screen left behind, alternate screen drawn, and the
        // keyboard pushed while on it. The repaint has to set the mode before
        // it paints, or switching screens afterwards would wipe the paint.
        let mut t = Terminal::new(40, 5).unwrap();
        t.write(b"$ vim notes\r\n");
        t.write(b"\x1b[?1049h\x1b[H\x1b[2Jediting notes\x1b[>1u");
        assert!(t.dec_mode(1049));

        let fresh = replayed(&t, 40, 5);
        assert!(fresh.dec_mode(1049), "alternate screen lost in the repaint");
        assert!(
            fresh.text().unwrap().contains("editing notes"),
            "alternate screen came back blank: {:?}",
            fresh.text().unwrap()
        );
        assert_eq!(fresh.kitty_keyboard_flags(), 1, "the editor's keyboard lost");
    }

    #[test]
    fn split_writes_match_single_write() {
        let mut whole = Terminal::new(20, 3).unwrap();
        whole.write(b"\x1b[1mbold text\x1b[0m");

        // A PTY delivers bytes in arbitrary chunks, including mid-escape.
        let mut chunked = Terminal::new(20, 3).unwrap();
        for chunk in [&b"\x1b"[..], b"[1mbo", b"ld te", b"xt\x1b[0m"] {
            chunked.write(chunk);
        }

        assert_eq!(whole.text().unwrap(), chunked.text().unwrap());
    }
}

/// Layout guards.
///
/// These structs are passed to C **by value**, so a field added upstream
/// silently corrupts every call. libghostty-vt has no stable ABI yet, so the
/// sizes are pinned here on purpose: when you re-vendor the library and these
/// fail, re-read the headers before touching the numbers.
///
/// Re-derive with:
/// ```sh
/// printf '#include <ghostty/vt.h>\n#include <stdio.h>\nint main(void){printf("%zu %zu %zu\\n",\
/// sizeof(GhosttyFormatterScreenExtra),sizeof(GhosttyFormatterTerminalExtra),\
/// sizeof(GhosttyFormatterTerminalOptions));}' > /tmp/s.c && \
///   cc /tmp/s.c -Ivendor/libghostty-vt/include \
///      vendor/libghostty-vt/libghostty-vt.a -o /tmp/s && /tmp/s
/// ```
#[cfg(test)]
mod abi {
    use super::ffi;

    #[test]
    fn struct_sizes_match_c_headers() {
        assert_eq!(std::mem::size_of::<ffi::ScreenExtra>(), 16, "ScreenExtra");
        assert_eq!(std::mem::size_of::<ffi::TerminalExtra>(), 32, "TerminalExtra");
        assert_eq!(std::mem::size_of::<ffi::FormatterOptions>(), 56, "FormatterOptions");
        assert_eq!(std::mem::size_of::<ffi::GridRef>(), 24, "GridRef");
        assert_eq!(std::mem::size_of::<ffi::Point>(), 24, "Point");
        assert_eq!(std::mem::size_of::<ffi::Selection>(), 64, "Selection");
    }
}

#[cfg(test)]
mod replies {
    use super::*;

    #[test]
    fn nothing_is_answered_unless_asked_to() {
        let mut t = Terminal::new(40, 5).unwrap();
        t.write(b"\x1b[6n");
        assert!(t.take_replies().is_empty());
    }

    #[test]
    fn the_cursor_position_is_answered() {
        let mut t = Terminal::new(40, 5).unwrap();
        t.answer_queries().unwrap();
        t.write(b"abc\x1b[6n");
        assert_eq!(t.take_replies(), b"\x1b[1;4R");
        assert!(t.take_replies().is_empty(), "collected once");
    }

    #[test]
    fn answers_survive_the_terminal_moving() {
        let mut t = Terminal::new(40, 5).unwrap();
        t.answer_queries().unwrap();
        let mut moved = vec![t];
        let t = &mut moved[0];
        t.write(b"\x1b[2;3H\x1b[6n");
        assert_eq!(t.take_replies(), b"\x1b[2;3R");
    }
}

#[cfg(test)]
mod title {
    use super::*;

    #[test]
    fn reads_the_title_set_by_osc() {
        let mut t = Terminal::new(40, 5).unwrap();
        assert_eq!(t.title(), "", "a fresh terminal has no title");

        // OSC 2 is what shells and editors use to name the window.
        t.write(b"\x1b]2;building orion\x07");
        assert_eq!(t.title(), "building orion");

        // A later title replaces the earlier one.
        t.write(b"\x1b]2;running tests\x07");
        assert_eq!(t.title(), "running tests");
    }

    #[test]
    fn title_survives_being_split_across_writes() {
        let mut t = Terminal::new(40, 5).unwrap();
        for chunk in [&b"\x1b]2;par"[..], b"tial ti", b"tle\x07"] {
            t.write(chunk);
        }
        assert_eq!(t.title(), "partial title");
    }
}
